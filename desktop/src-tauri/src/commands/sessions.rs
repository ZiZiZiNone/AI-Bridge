//! W5 临时会话面板的命令接线（C1–C5：签发 / 续期 / 释放 / 吊销 / revoke_all）。
//!
//! 复用而非重写：签发的多账号消歧、真会话 → 信封归一、scope/TTL 强制、审计 fail-closed
//! 全部走 `ab-mcp::McpService`（其内部转 `ab-broker`）；本层不写第二份规则。
//! 单会话吊销走 `ab-broker::revoke`（MCP 工具面无该动作，属面板动作）。

use ab_broker::GrantStatus;
use ab_mcp::{AcquireParams, AcquireResult, ListResult, McpService, McpTarget, SessionSummary};
use ab_state::SystemClock;
use serde::{Deserialize, Serialize};

use super::{now_unix_secs, CmdResult};
use crate::host::{HostServices, HOST_WHO};

/// 签发入参（命令面搬运；结构与 MCP 工具面同构，McpTarget/McpScope 直接复用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueParams {
    pub target: McpTarget,
    pub scope: ab_mcp::McpScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_ref: Option<String>,
}

impl From<&IssueParams> for AcquireParams {
    fn from(params: &IssueParams) -> Self {
        AcquireParams {
            target: params.target.clone(),
            scope: params.scope.clone(),
            ttl_secs: params.ttl_secs,
            account_ref: params.account_ref.clone(),
        }
    }
}

/// 桌面侧 MCP 服务实例（与 MCP server 共享同一 broker / 会话查询）。
fn mcp(services: &HostServices) -> McpService {
    McpService::new(
        services.broker.clone(),
        services.state.clone(),
        Arc::new(SystemClock),
        Arc::new(services.entropy),
        HOST_WHO,
    )
}

use std::sync::Arc;

/// C1.2 / C2.1 / C4.1 签发临时会话（真会话由 broker 代持，返回一次性句柄）。
pub fn issue(services: &HostServices, params: &IssueParams) -> CmdResult<AcquireResult> {
    mcp(services)
        .acquire_session(&params.into())
        .map_err(|error| error.to_string())
}

/// A2.2 临时会话总览（剩余 TTL / scope / 来源；不含句柄）。
pub fn list(services: &HostServices) -> CmdResult<ListResult> {
    mcp(services)
        .list_sessions()
        .map_err(|error| error.to_string())
}

/// C2.2 续期（上限 = 签发 + 24h，core 强制）。
pub fn renew(
    services: &HostServices,
    session_id: &str,
    ttl_secs: Option<u64>,
) -> CmdResult<SessionSummary> {
    mcp(services)
        .renew_session(&ab_mcp::SessionIdParams {
            session_id: session_id.to_string(),
            ttl_secs,
        })
        .map_err(|error| error.to_string())
}

/// C2.2 显式释放（只失效我方凭证，默认不登出第三方）。
pub fn release(services: &HostServices, session_id: &str) -> CmdResult<()> {
    mcp(services)
        .release_session(session_id)
        .map_err(|error| error.to_string())
}

/// C5.1 单会话吊销（面板动作；审计由 ab-broker 记账）。
pub fn revoke(services: &HostServices, session_id: &str) -> CmdResult<()> {
    services
        .broker
        .revoke(HOST_WHO, session_id, now_unix_secs())
        .map_err(|error| error.to_string())
}

/// C5.1 一键全部吊销。
pub fn revoke_all(services: &HostServices) -> CmdResult<usize> {
    mcp(services)
        .revoke_all()
        .map_err(|error| error.to_string())
        .map(|result| result.revoked)
}

/// C2.2 空闲 / TTL 到期清理（ab-broker 的 sweep；面板刷新或定时器触发点）。
pub fn sweep(services: &HostServices) -> CmdResult<Vec<String>> {
    services
        .broker
        .sweep(HOST_WHO, now_unix_secs())
        .map_err(|error| error.to_string())
}

/// 活跃授权（候选条 C5 快捷吊销的数据源之一）。
pub fn active_grants(services: &HostServices) -> CmdResult<Vec<GrantStatus>> {
    let grants = services
        .broker
        .list(now_unix_secs())
        .map_err(|error| error.to_string())?;
    Ok(grants
        .into_iter()
        .filter(|grant| grant.state == ab_broker::GrantState::Active)
        .collect())
}

/* ---------------- Tauri 命令包装（只转发） ---------------- */

#[tauri::command]
pub fn session_issue(
    state: tauri::State<'_, HostServices>,
    params: IssueParams,
) -> CmdResult<AcquireResult> {
    issue(&state, &params)
}

#[tauri::command]
pub fn session_list(state: tauri::State<'_, HostServices>) -> CmdResult<ListResult> {
    list(&state)
}

#[tauri::command]
pub fn session_renew(
    state: tauri::State<'_, HostServices>,
    session_id: String,
    ttl_secs: Option<u64>,
) -> CmdResult<SessionSummary> {
    renew(&state, &session_id, ttl_secs)
}

#[tauri::command]
pub fn session_release(state: tauri::State<'_, HostServices>, session_id: String) -> CmdResult<()> {
    release(&state, &session_id)
}

#[tauri::command]
pub fn session_revoke(state: tauri::State<'_, HostServices>, session_id: String) -> CmdResult<()> {
    revoke(&state, &session_id)
}

#[tauri::command]
pub fn session_revoke_all(state: tauri::State<'_, HostServices>) -> CmdResult<usize> {
    revoke_all(&state)
}

#[tauri::command]
pub fn session_sweep(state: tauri::State<'_, HostServices>) -> CmdResult<Vec<String>> {
    sweep(&state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::state::{self, EndpointDto};
    use crate::host::{HostPaths, HostServices, OfficialVerifier};
    use ab_audit::{AuditFilter, InMemoryAuditSink};
    use ab_contract::{
        Cookie, Payload, SameSite, SessionKind, SessionRecord, Source, SourceType, Target,
        TargetKind,
    };
    use ab_state::{Endpoint, EndpointKind};
    use std::sync::Arc;

    fn test_services() -> HostServices {
        let root = std::env::temp_dir().join(format!(
            "aibridge-sessions-test-{}-{}",
            std::process::id(),
            now_unix_secs()
        ));
        let _ = std::fs::remove_dir_all(&root);
        HostServices::build_with(
            "pc-test",
            HostPaths { root },
            OfficialVerifier::new(None, None),
            Arc::new(InMemoryAuditSink::new()),
        )
    }

    fn seed_session(services: &HostServices, account_ref: &str, domain: &str) {
        let endpoint = EndpointDto {
            kind: EndpointKind::Browser,
            id: "chromium".into(),
        };
        services.source.ingest(
            Endpoint::from(&endpoint),
            vec![SessionRecord {
                account_ref: account_ref.into(),
                target: Target {
                    kind: TargetKind::Web,
                    domain: Some(domain.into()),
                    app_id: None,
                },
                kind: SessionKind::Cookie,
                payload: Payload {
                    cookies: vec![Cookie {
                        name: "sid".into(),
                        value: "real-session-secret".into(),
                        domain: Some(domain.into()),
                        path: Some("/".into()),
                        secure: Some(true),
                        http_only: Some(true),
                        same_site: Some(SameSite::Lax),
                        expires_at: None,
                    }],
                    ..Payload::default()
                },
                expires_at: None,
                source: Source {
                    source_type: SourceType::Extension,
                    id: "chromium".into(),
                },
                client_env: None,
            }],
        );
        state::capture(&services, &endpoint).expect("捕获失败");
    }

    fn issue_params(domain: &str) -> IssueParams {
        IssueParams {
            target: McpTarget {
                kind: "web".into(),
                domain: Some(domain.into()),
                app_id: None,
            },
            scope: ab_mcp::McpScope {
                domain_allowlist: vec![domain.into()],
                read_only: true,
                single_use: false,
            },
            ttl_secs: Some(3600),
            account_ref: None,
        }
    }

    #[test]
    fn issue_list_release_revoke_all_go_through_broker() {
        let services = test_services();
        seed_session(&services, "acc-1", "github.com");

        // 签发：真会话代持，句柄一次性返回（C2.1）。
        let grant = issue(&services, &issue_params("github.com")).expect("签发失败");
        assert!(!grant.handle.is_empty());

        // 总览不含句柄（A2.2 / 秘密不进 UI 通道）。
        let listed = list(&services).expect("总览失败");
        assert_eq!(listed.sessions.len(), 1);
        assert_eq!(listed.sessions[0].session_id, grant.session_id);
        let debug = format!("{listed:?}");
        assert!(!debug.contains(&grant.handle), "总览不得携带句柄");

        // 续期（C2.2）。
        let renewed = renew(&services, &grant.session_id, Some(7200)).expect("续期失败");
        assert!(renewed.remaining_ttl_secs > 3600);

        // 释放（C2.2）→ 不再是活跃授权。
        release(&services, &grant.session_id).expect("释放失败");
        assert!(active_grants(&services).expect("活跃授权失败").is_empty());

        // C5.1：签发两个，一键全部吊销。
        let second = issue(&services, &issue_params("github.com")).expect("签发失败");
        let third = issue(&services, &issue_params("github.com")).expect("签发失败");
        assert_ne!(second.session_id, third.session_id);
        assert_eq!(revoke_all(&services).expect("吊销失败"), 2);
        assert!(active_grants(&services).expect("活跃授权失败").is_empty());

        // 一切动作进审计流（C3.1，ab-broker fail-closed 记账）。
        let events = services
            .audit
            .query(&AuditFilter::default())
            .expect("查询失败");
        assert!(events.iter().any(|event| event.action == ab_audit::AuditAction::Issue));
        assert!(events.iter().any(|event| event.action == ab_audit::AuditAction::Release));
        assert!(events
            .iter()
            .any(|event| event.action == ab_audit::AuditAction::RevokeAll));
    }

    #[test]
    fn scope_and_target_rules_stay_in_core() {
        let services = test_services();
        seed_session(&services, "acc-1", "github.com");

        // 目标不在域名白名单 → broker 拒绝（规则只在 core 一份）。
        let mut params = issue_params("github.com");
        params.scope.domain_allowlist = vec!["example.com".into()];
        let error = issue(&services, &params).unwrap_err();
        assert!(error.contains("目标不在域名白名单内"), "实际：{error}");

        // 同站多账号并存且未指定 account_ref → ab-mcp 消歧规则报候选。
        seed_session(&services, "acc-2", "github.com");
        let error = issue(&services, &issue_params("github.com")).unwrap_err();
        assert!(error.contains("请指定 account_ref"), "实际：{error}");
    }

    #[test]
    fn single_revoke_and_sweep_are_forwarded() {
        let services = test_services();
        seed_session(&services, "acc-1", "github.com");
        let grant = issue(&services, &issue_params("github.com")).expect("签发失败");

        revoke(&services, &grant.session_id).expect("吊销失败");
        assert!(active_grants(&services).expect("活跃授权失败").is_empty());
        assert!(sweep(&services).expect("清理失败").is_empty());
    }
}
