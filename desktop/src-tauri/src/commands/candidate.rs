//! W2 候选条的数据源命令（B5 一次性复制候选 · B6.2 窗口/域名匹配 · A4.1 多账号 · C5 快捷吊销）。
//!
//! 数据流 1（docs/架构与契约.md §5）：`ab-vault` 查询 + `ab-state` 匹配 → 结构化候选。
//! 本层只组装三路候选（条目 / 会话 / 活跃授权），不做业务判断；
//! **秘密不出本层**：条目只出 `EntryView`（无 secret / totp_seed），会话只出矩阵行（无 payload）。
//!
//! core 缺口（见 README）：条目与域名的匹配规则（B6.2）core 无公开 API
//! （`ab-vault` 无检索、`ab-state` 无按域名筛选）——本层的 [`url_matches_domain`] 是
//! 机械化占位实现，规则定稿后应下沉 core，此处只留一处实现。

use ab_broker::SessionStatus;
use ab_state::{MatrixFilters, MatrixRow};
use serde::{Deserialize, Serialize};

use super::library::EntryView;
use super::{sessions, CmdResult};
use crate::host::HostState;

/// 候选条数据（跟随鼠标小面板的一次取数）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateData {
    /// 库条目候选（A4.1 多账号并存时多行，含 account_ref 供消歧）。
    pub entries: Vec<EntryView>,
    /// 会话候选（账号 × 端 × 到期；无 payload）。
    pub sessions: Vec<MatrixRow>,
    /// 活跃临时授权（C5 快捷吊销入口）。
    pub grants: Vec<SessionStatus>,
}

/// 域名匹配（B6.2 机械占位，见文件头 core 缺口）：
/// 条目 URL 的主机与线索相等，或为线索的子域（`login.example.com` 命中 `example.com`）。
pub fn url_matches_domain(url: &str, domain: &str) -> bool {
    if domain.is_empty() {
        return false;
    }
    let host = url
        .split_once("://")
        .map(|(scheme, rest)| {
            if scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https") {
                rest
            } else {
                url // 非 web scheme 时退化为整串（不猜结构）
            }
        })
        .unwrap_or(url);
    let host = host.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.split('@').next_back().unwrap_or("");
    let host = host.split(':').next().unwrap_or("");
    let host = host.trim_start_matches('.').to_ascii_lowercase();
    let domain = domain.trim_start_matches('.').to_ascii_lowercase();
    host == domain || host.ends_with(&format!(".{domain}"))
}

/// 取候选（锁定态条目为空：候选条不在锁定态吐任何库内容）。
pub fn data(services: &HostState, domain: &str) -> CmdResult<CandidateData> {
    let entries = if super::library::is_unlocked(services) {
        super::library::list_entries(services)?
            .into_iter()
            .filter(|entry| entry.url.as_deref().is_some_and(|url| url_matches_domain(url, domain)))
            .collect()
    } else {
        Vec::new()
    };

    let sessions = services
        .state
        .matrix(&MatrixFilters::default())
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|row| row.target.domain.as_deref() == Some(domain))
        .collect();

    let grants = sessions::active_grants(services)?
        .into_iter()
        .filter(|grant| grant.target.domain.as_deref() == Some(domain))
        .collect();

    Ok(CandidateData {
        entries,
        sessions,
        grants,
    })
}

/* ---------------- Tauri 命令包装（只转发） ---------------- */

#[tauri::command]
pub fn candidate_data(
    state: tauri::State<'_, HostState>,
    domain: String,
) -> CmdResult<CandidateData> {
    data(&state, &domain)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::library;
    use crate::commands::state::{self, EndpointDto};
    use crate::host::{HostLayout, HostState, HostSignatureVerifier};
    use ab_audit::InMemoryAuditSink;
    use ab_contract::{
        Cookie, CredentialKind, Payload, SameSite, SessionKind, SessionRecord, Source, SourceType,
        Target, TargetKind,
    };
    use ab_state::{Endpoint, EndpointKind};
    use std::sync::Arc;

    fn test_services() -> HostState {
        let root = crate::host::test_root("aibridge-candidate-test");
        let _ = std::fs::remove_dir_all(&root);
        HostState::build_with(
            "pc-test",
            HostLayout { root },
            HostSignatureVerifier::new(None, None),
            Arc::new(InMemoryAuditSink::new()),
        )
    }

    /// 秘密哨兵：足够独特，出现即证明秘密真的泄漏（而非字段名撞词）。
    const SECRET_SENTINEL: &str = "SENTINEL-9f3c-secret-value";

    fn entry_input(title: &str, url: &str) -> library::EntryInput {
        library::EntryInput {
            title: title.into(),
            account_ref: format!("acc-{title}"),
            kind: CredentialKind::Password,
            username: Some(format!("{title}@example.com")),
            secret: Some(SECRET_SENTINEL.into()),
            totp_seed: None,
            url: Some(url.into()),
            note: None,
        }
    }

    #[test]
    fn domain_matching_is_mechanical_and_single_sourced() {
        assert!(url_matches_domain("https://github.com/login", "github.com"));
        assert!(url_matches_domain("https://login.github.com/", "github.com"));
        assert!(url_matches_domain("https://github.com", "GitHub.com"));
        assert!(!url_matches_domain("https://evil-github.com", "github.com"));
        assert!(!url_matches_domain("https://example.com", ""));
    }

    #[test]
    fn candidates_join_vault_state_and_grants_without_secrets() {
        let services = test_services();
        library::create(&services, "个人", "主口令").expect("建库失败");
        library::add_entry(&services, entry_input("work", "https://github.com/login"))
            .expect("新建失败");
        library::add_entry(&services, entry_input("mail", "https://mail.example.com"))
            .expect("新建失败");

        // 会话（来自扩展捕获）。
        let endpoint = EndpointDto {
            kind: EndpointKind::Browser,
            id: "chromium".into(),
        };
        services.source.ingest(
            Endpoint::from(&endpoint),
            vec![SessionRecord {
                account_ref: "acc-work".into(),
                target: Target {
                    kind: TargetKind::Web,
                    domain: Some("github.com".into()),
                    app_id: None,
                },
                kind: SessionKind::Cookie,
                payload: Payload {
                    cookies: vec![Cookie {
                        name: "sid".into(),
                        value: "cookie-secret".into(),
                        domain: Some("github.com".into()),
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

        let data = data(&services, "github.com").expect("取数失败");
        assert_eq!(data.entries.len(), 1, "只有 github 条目命中");
        assert_eq!(data.entries[0].title, "work");
        assert_eq!(data.sessions.len(), 1, "会话按域名命中");
        assert!(data.grants.is_empty());

        // 秘密纪律：`EntryView` 只出 `has_secret` / `has_totp` 布尔标记，绝不带秘密原文。
        // 断言用哨兵值而非 "secret" 子串——后者会与 `has_secret` 字段名撞词（假阳性）。
        let debug = format!("{data:?}");
        assert!(!debug.contains(SECRET_SENTINEL), "候选数据不得含秘密原文");
        assert!(!debug.contains("cookie-secret"), "候选数据不得含 cookie 值");
        assert!(debug.contains("has_secret: true"), "带密钥的条目应标记 has_secret");
    }

    #[test]
    fn locked_state_yields_no_vault_entries() {
        let services = test_services();
        library::create(&services, "个人", "主口令").expect("建库失败");
        library::add_entry(&services, entry_input("work", "https://github.com/login"))
            .expect("新建失败");
        library::lock(&services).expect("锁定失败");

        let data = data(&services, "github.com").expect("取数失败");
        assert!(data.entries.is_empty(), "锁定态不得吐库内容");
    }
}
