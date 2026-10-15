//! W6 审计面板的命令接线（C3.1 审计流 · C3.2 筛选导出的数据源）。
//!
//! 只转发 `ab-audit`：筛选语义（`AuditFilter`）在 core，本层不重写一份。
//! 「加密导出」（C3.2）core 无 API（见 README 缺口清单），未接线。

use ab_audit::AuditEvent;

use super::{AuditFilterDto, CmdResult};
use crate::host::HostState;

/// C3.1/C3.2 查询（筛选字段精确匹配；`None` = 不限）。
pub fn query(services: &HostState, filter: &AuditFilterDto) -> CmdResult<Vec<AuditEvent>> {
    services
        .audit
        .query(&filter.into())
        .map_err(|error| error.to_string())
}

/// 已入账条数（W6 计数展示）。
pub fn count(services: &HostState) -> CmdResult<usize> {
    services.audit.count().map_err(|error| error.to_string())
}

/* ---------------- Tauri 命令包装（只转发） ---------------- */

#[tauri::command]
pub fn audit_query(
    state: tauri::State<'_, HostState>,
    filter: AuditFilterDto,
) -> CmdResult<Vec<AuditEvent>> {
    query(&state, &filter)
}

#[tauri::command]
pub fn audit_count(state: tauri::State<'_, HostState>) -> CmdResult<usize> {
    count(&state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::library;
    use crate::host::{HostLayout, HostState, HostSignatureVerifier};
    use ab_audit::{AuditAction, InMemoryAuditSink};
    use std::sync::Arc;

    #[test]
    fn query_forwards_filter_to_core_sink() {
        let root = crate::host::test_root("aibridge-audit-test");
        let _ = std::fs::remove_dir_all(&root);
        let services = HostState::build_with(
            "pc-test",
            HostLayout { root },
            HostSignatureVerifier::new(None, None),
            Arc::new(InMemoryAuditSink::new()),
        );

        library::create(&services, "个人", "主口令").expect("建库失败");
        assert!(count(&services).expect("计数失败") >= 2);

        let unlock_only = query(
            &services,
            &AuditFilterDto {
                action: Some(AuditAction::Unlock),
                ..AuditFilterDto::default()
            },
        )
        .expect("查询失败");
        assert!(!unlock_only.is_empty());
        assert!(unlock_only
            .iter()
            .all(|event| event.action == AuditAction::Unlock));
    }
}
