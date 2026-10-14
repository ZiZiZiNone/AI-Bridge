//! W4 状态清单面板的命令接线（A2 会话矩阵 · A3 健康 · A7 覆盖快照/回滚 · B1.3）。
//!
//! 全部转发 `ab-state`（矩阵筛选/排序、健康分级、快照与回滚的规则只在 core 一份）；
//! 本层不判定「将过期 / 已过期」——阈值与分级来自 `HealthPolicy`。
//! 矩阵行不含 payload（cookie 原文不经过 UI 层，core 已保证）。

use ab_audit::{AuditAction, AuditResult};
use ab_state::{
    CaptureReport, Endpoint, EndpointKind, HealthReport, MatrixFilters, MatrixRow,
    RollbackReport, SnapshotMeta,
};
use serde::{Deserialize, Serialize};

use super::{record, CmdResult};
use crate::host::HostServices;

/// 端标识入参（命令面搬运）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EndpointDto {
    pub kind: EndpointKind,
    pub id: String,
}

impl From<&EndpointDto> for Endpoint {
    fn from(dto: &EndpointDto) -> Self {
        Endpoint::new(dto.kind, dto.id.clone())
    }
}

/// A2.1/A2.3 会话矩阵（账号 × 端 × 来源 × 到期，可筛选）。
pub fn matrix(services: &HostServices, filters: MatrixFilters) -> CmdResult<Vec<MatrixRow>> {
    services
        .state
        .matrix(&filters)
        .map_err(|error| error.to_string())
}

/// A3.1 会话健康（到期倒计时与分级）。
pub fn health(services: &HostServices) -> CmdResult<HealthReport> {
    services.state.health().map_err(|error| error.to_string())
}

/// A7.2 快照列表（最新在前）。
pub fn snapshots(services: &HostServices) -> CmdResult<Vec<SnapshotMeta>> {
    services
        .state
        .list_snapshots()
        .map_err(|error| error.to_string())
}

/// A7.2 一键回滚（写账 fail-closed）。
pub fn rollback(services: &HostServices, snapshot_id: &str) -> CmdResult<RollbackReport> {
    let report = services
        .state
        .rollback(snapshot_id)
        .map_err(|error| error.to_string())?;
    record(
        services,
        AuditAction::Restore,
        AuditResult::Ok,
        None,
        None,
        &[
            ("target", "session-rollback"),
            ("snapshot", snapshot_id),
        ],
    )?;
    Ok(report)
}

/// A7.1 覆盖前快照 + 捕获某端会话（会话来自 `SessionSource` 注入面）。
pub fn capture(services: &HostServices, endpoint: &EndpointDto) -> CmdResult<CaptureReport> {
    let endpoint = Endpoint::from(endpoint);
    let report = services
        .state
        .capture(&endpoint)
        .map_err(|error| error.to_string())?;
    record(
        services,
        AuditAction::Capture,
        AuditResult::Ok,
        None,
        None,
        &[("endpoint", &endpoint.id)],
    )?;
    Ok(report)
}

/* ---------------- Tauri 命令包装（只转发） ---------------- */

#[tauri::command]
pub fn state_matrix(
    state: tauri::State<'_, HostServices>,
    filters: MatrixFilters,
) -> CmdResult<Vec<MatrixRow>> {
    matrix(&state, filters)
}

#[tauri::command]
pub fn state_health(state: tauri::State<'_, HostServices>) -> CmdResult<HealthReport> {
    health(&state)
}

#[tauri::command]
pub fn state_snapshots(state: tauri::State<'_, HostServices>) -> CmdResult<Vec<SnapshotMeta>> {
    snapshots(&state)
}

#[tauri::command]
pub fn state_rollback(
    state: tauri::State<'_, HostServices>,
    snapshot_id: String,
) -> CmdResult<RollbackReport> {
    rollback(&state, &snapshot_id)
}

#[tauri::command]
pub fn state_capture(
    state: tauri::State<'_, HostServices>,
    endpoint: EndpointDto,
) -> CmdResult<CaptureReport> {
    capture(&state, &endpoint)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{HostPaths, HostServices, OfficialVerifier};
    use ab_audit::{AuditFilter, InMemoryAuditSink};
    use ab_contract::{
        Cookie, Payload, SameSite, SessionKind, SessionRecord, Source, SourceType, Target,
        TargetKind,
    };
    use std::sync::Arc;

    fn test_services() -> HostServices {
        let root = std::env::temp_dir().join(format!(
            "aibridge-state-test-{}-{}",
            std::process::id(),
            crate::commands::now_unix_secs()
        ));
        let _ = std::fs::remove_dir_all(&root);
        HostServices::build_with(
            "pc-test",
            HostPaths { root },
            OfficialVerifier::new(None, None),
            Arc::new(InMemoryAuditSink::new()),
        )
    }

    fn record_of(account_ref: &str, domain: &str, expires_at: Option<String>) -> SessionRecord {
        SessionRecord {
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
                    value: "secret-cookie".into(),
                    domain: Some(domain.into()),
                    path: Some("/".into()),
                    secure: Some(true),
                    http_only: Some(true),
                    same_site: Some(SameSite::Lax),
                    expires_at: None,
                }],
                ..Payload::default()
            },
            expires_at,
            source: Source {
                source_type: SourceType::Extension,
                id: "chromium".into(),
            },
            client_env: None,
        }
    }

    #[test]
    fn capture_matrix_health_and_rollback_use_core_rules() {
        let services = test_services();
        let endpoint = EndpointDto {
            kind: EndpointKind::Browser,
            id: "chromium".into(),
        };

        // 会话经 SessionSource 注入面进入（A7.1 覆盖前快照在 core 内完成）。
        services.source.ingest(
            Endpoint::from(&endpoint),
            vec![
                record_of("acc-a", "github.com", None),
                record_of(
                    "acc-b",
                    "example.com",
                    Some("2020-01-01T00:00:00Z".into()),
                ),
            ],
        );
        let report = capture(&services, &endpoint).expect("捕获失败");
        assert_eq!(report.captured_entries, 2);

        // 矩阵：筛选 + 分级全部来自 ab-state（无到期 = 活跃；已过期如实分级）。
        let rows = matrix(&services, MatrixFilters::default()).expect("矩阵失败");
        assert_eq!(rows.len(), 2);
        let expired = rows
            .iter()
            .find(|row| row.account_ref == "acc-b")
            .expect("应有 acc-b");
        assert_eq!(expired.health, ab_state::HealthLevel::Expired);
        let active = rows
            .iter()
            .find(|row| row.account_ref == "acc-a")
            .expect("应有 acc-a");
        assert_eq!(active.health, ab_state::HealthLevel::Active);
        let debug = format!("{rows:?}");
        assert!(!debug.contains("secret-cookie"), "矩阵不得带 payload");

        let health = health(&services).expect("健康失败");
        assert_eq!(health.expired, 1);
        assert_eq!(health.active, 1);

        // A7.1 每次覆盖前生成全量快照：第一次捕获留下「捕获前（空）」快照。
        let snapshot_list = snapshots(&services).expect("快照列表失败");
        assert_eq!(snapshot_list.len(), 1, "capture 前的全量快照应有一份");

        // 覆盖为只剩 acc-a（第二次捕获再留一份「覆盖前（2 条）」快照）。
        services
            .source
            .ingest(Endpoint::from(&endpoint), vec![record_of("acc-a", "github.com", None)]);
        capture(&services, &endpoint).expect("二次捕获失败");
        assert_eq!(matrix(&services, MatrixFilters::default()).expect("矩阵失败").len(), 1);

        // A7.2 回滚到「覆盖前」快照，逐条还原。
        let snapshot_list = snapshots(&services).expect("快照列表失败");
        assert_eq!(snapshot_list.len(), 2, "最新在前");
        let rollback_report = rollback(&services, &snapshot_list[0].id).expect("回滚失败");
        assert_eq!(rollback_report.restored_entries, 2);
        assert_eq!(matrix(&services, MatrixFilters::default()).expect("矩阵失败").len(), 2);

        // 回滚入审计（fail-closed 出口）。
        let events = services.audit.query(&AuditFilter::default()).expect("查询失败");
        assert!(events
            .iter()
            .any(|event| event.action == AuditAction::Restore));
    }

    #[test]
    fn matrix_filters_are_forwarded_to_core() {
        let services = test_services();
        let endpoint = EndpointDto {
            kind: EndpointKind::Browser,
            id: "chromium".into(),
        };
        services.source.ingest(
            Endpoint::from(&endpoint),
            vec![record_of("acc-a", "github.com", None)],
        );
        capture(&services, &endpoint).expect("捕获失败");

        let hit = matrix(
            &services,
            MatrixFilters {
                account_ref: Some("acc-a".into()),
                ..MatrixFilters::default()
            },
        )
        .expect("矩阵失败");
        assert_eq!(hit.len(), 1);

        let miss = matrix(
            &services,
            MatrixFilters {
                account_ref: Some("nobody".into()),
                ..MatrixFilters::default()
            },
        )
        .expect("矩阵失败");
        assert!(miss.is_empty());
    }
}
