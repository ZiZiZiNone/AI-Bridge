//! 覆盖前快照 / 快照读取 / 回滚（功能点 A7.1、A7.2）。
//!
//! 一致性口径：写入存储的条目集合一律先经身份规范化（[`canonicalize`]），
//! 快照按同一序保存，回滚按原样写回——因此「回滚后状态 == 该快照条目」是逐条相等的。

use crate::error::StateError;
use crate::model::{
    canonicalize, CaptureReport, Endpoint, RollbackReport, SessionEntry, Snapshot, SnapshotMeta,
};
use crate::{Clock, SessionSource, StateService, StateStore};

impl<S: StateStore, C: Clock, R: SessionSource> StateService<S, C, R> {
    /// 捕获某个端的会话：**先**把覆盖前的旧状态整份落成快照，再写入新读到的会话。
    ///
    /// - 只替换该端（同 kind + id）的旧条目，其他端的会话原样保留；
    /// - 写入前逐条校验（契约 `validate` + 状态层前置）；校验失败则不动存储、不留快照；
    /// - 快照保存的是覆盖前的**全量**状态，故回滚可整份还原。
    pub fn capture(&self, endpoint: &Endpoint) -> Result<CaptureReport, StateError> {
        let records = self.source.read(endpoint)?;
        let mut incoming: Vec<SessionEntry> = Vec::with_capacity(records.len());
        for record in records {
            let entry = SessionEntry::new(endpoint.clone(), record);
            entry.validate()?;
            incoming.push(entry);
        }
        let captured_entries = incoming.len();

        let previous = self.store.load_sessions()?;
        let created_at = self.clock.now_unix_secs();
        let id = self.next_snapshot_id(created_at)?;
        let snapshot = Snapshot {
            id,
            created_at,
            entries: previous.clone(),
        };
        let meta = snapshot.meta();
        self.store.save_snapshot(snapshot)?;

        // 被覆盖 = 该端在旧集合中的条目数（filter 掉的那部分；审查报告 P2：
        // 原实现统计的是滤除后保留的 merged.len()，与 model.rs 语义相反）。
        let replaced_entries = previous
            .iter()
            .filter(|entry| &entry.endpoint == endpoint)
            .count();
        let mut merged: Vec<SessionEntry> = previous
            .into_iter()
            .filter(|entry| &entry.endpoint != endpoint)
            .collect();
        merged.extend(incoming);
        self.store.replace_sessions(canonicalize(merged))?;

        Ok(CaptureReport {
            snapshot: meta,
            captured_entries,
            replaced_entries,
        })
    }

    /// 按 id 读取快照正文；不存在报 [`StateError::SnapshotNotFound`]。
    pub fn snapshot(&self, id: &str) -> Result<Snapshot, StateError> {
        match self.store.load_snapshot(id)? {
            Some(snapshot) => Ok(snapshot),
            None => Err(StateError::SnapshotNotFound(id.to_string())),
        }
    }

    /// 列出全部快照，最新在前（同一秒内按 id 降序）。
    pub fn list_snapshots(&self) -> Result<Vec<SnapshotMeta>, StateError> {
        let mut snapshots = self.store.list_snapshots()?;
        snapshots.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
        Ok(snapshots)
    }

    /// 按快照整份回滚：写入快照条目，回滚后状态与该快照逐条一致。
    pub fn rollback(&self, id: &str) -> Result<RollbackReport, StateError> {
        let snapshot = self.snapshot(id)?;
        let restored_entries = snapshot.entries.len();
        let replaced_entries = self.store.load_sessions()?.len();
        self.store.replace_sessions(snapshot.entries)?;
        Ok(RollbackReport {
            snapshot_id: id.to_string(),
            restored_entries,
            replaced_entries,
        })
    }

    /// 快照 id：`snap-<纪元秒>-<序号>`；同一秒内序号递增，且不与已有 id 冲突。
    fn next_snapshot_id(&self, created_at: i64) -> Result<String, StateError> {
        let existing = self.store.list_snapshots()?;
        let mut ordinal = existing.len() + 1;
        loop {
            let candidate = format!("snap-{created_at}-{ordinal:03}");
            if !existing.iter().any(|meta| meta.id == candidate) {
                return Ok(candidate);
            }
            ordinal += 1;
            if ordinal > 1_000_000 {
                return Err(StateError::Store("快照 id 序号耗尽".to_string()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::HealthPolicy;
    use crate::model::EndpointKind;
    use crate::store::InMemoryStateStore;
    use crate::test_support::*;

    fn policy() -> HealthPolicy {
        HealthPolicy {
            warn_within_secs: 100,
        }
    }

    fn chromium() -> Endpoint {
        Endpoint::new(EndpointKind::Browser, "chromium")
    }

    fn seeded() -> (InMemoryStateStore, Vec<SessionEntry>) {
        let previous = vec![
            entry(
                EndpointKind::Browser,
                "chromium",
                "acc-a",
                "github.com",
                Some(T_PLUS_DAY),
            ),
            entry(
                EndpointKind::Browser,
                "firefox",
                "acc-b",
                "gitlab.com",
                Some(T_PLUS_DAY),
            ),
        ];
        (store_with(previous.clone()), canonicalize(previous))
    }

    #[test]
    fn capture_snapshots_previous_state_and_rollback_restores_it_exactly() {
        let (store, before) = seeded();
        let incoming = vec![
            record("acc-a", "github.com", Some(T_PLUS_DAY)),
            record("acc-new", "github.com", Some(T_PLUS_DAY)),
        ];
        let svc = service(store.clone(), incoming, BASE, policy());

        let report = svc.capture(&chromium()).expect("capture 失败");
        assert_eq!(report.snapshot.entry_count, 2);
        assert_eq!(report.snapshot.created_at, BASE);
        assert_eq!(report.captured_entries, 2);
        assert_eq!(report.replaced_entries, 1); // 只有 chromium 的旧会话被覆盖

        let after_capture = store.load_sessions().expect("读取失败");
        assert_eq!(after_capture.len(), 3);
        assert_eq!(
            after_capture
                .iter()
                .filter(|entry| entry.endpoint.id == "chromium")
                .count(),
            2
        );
        assert!(after_capture
            .iter()
            .any(|entry| entry.endpoint.id == "firefox"));

        let snapshot = svc.snapshot(&report.snapshot.id).expect("读快照失败");
        assert_eq!(snapshot.entries, before);

        let rollback = svc.rollback(&report.snapshot.id).expect("回滚失败");
        assert_eq!(rollback.snapshot_id, report.snapshot.id);
        assert_eq!(rollback.restored_entries, 2);
        assert_eq!(rollback.replaced_entries, 3);

        let restored = store.load_sessions().expect("读取失败");
        assert_eq!(restored, before);
        assert_eq!(restored, snapshot.entries);
    }

    #[test]
    fn capture_replaces_only_the_target_endpoint() {
        let (store, _) = seeded();
        let svc = service(store.clone(), vec![record("acc-a", "github.com", None)], BASE, policy());

        let _ = svc.capture(&chromium()).expect("capture 失败");

        let firefox: Vec<SessionEntry> = store
            .load_sessions()
            .expect("读取失败")
            .into_iter()
            .filter(|entry| entry.endpoint.id == "firefox")
            .collect();
        assert_eq!(firefox.len(), 1);
        assert_eq!(
            firefox[0].record.expires_at.as_deref(),
            Some(T_PLUS_DAY),
            "其他端条目必须原样保留"
        );
    }

    #[test]
    fn second_capture_rollback_restores_the_state_after_first_capture() {
        let (store, _) = seeded();
        let first = service(
            store.clone(),
            vec![record("acc-a", "github.com", Some(T_PLUS_DAY))],
            BASE,
            policy(),
        );
        let report_first = first.capture(&chromium()).expect("capture 失败");
        let state_after_first = store.load_sessions().expect("读取失败");

        let second = service(
            store.clone(),
            vec![record("acc-b", "github.com", Some(T_PLUS_DAY))],
            BASE,
            policy(),
        );
        let report_second = second.capture(&chromium()).expect("capture 失败");

        assert_ne!(report_first.snapshot.id, report_second.snapshot.id);
        let listed = second.list_snapshots().expect("列快照失败");
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id, report_second.snapshot.id); // 同一秒内后创建在前

        let rollback = second.rollback(&report_second.snapshot.id).expect("回滚失败");
        assert_eq!(rollback.restored_entries, state_after_first.len());
        assert_eq!(store.load_sessions().expect("读取失败"), state_after_first);
    }

    #[test]
    fn capture_on_empty_store_is_snapshottable() {
        let store = InMemoryStateStore::new();
        let svc = service(
            store.clone(),
            vec![record("acc-a", "github.com", None)],
            BASE,
            policy(),
        );

        let report = svc.capture(&chromium()).expect("capture 失败");
        assert_eq!(report.snapshot.entry_count, 0);
        assert_eq!(report.replaced_entries, 0);

        let rollback = svc.rollback(&report.snapshot.id).expect("回滚失败");
        assert_eq!(rollback.restored_entries, 0);
        assert!(store.load_sessions().expect("读取失败").is_empty());
    }

    #[test]
    fn capture_rejects_invalid_input_without_touching_store() {
        let store = InMemoryStateStore::new();

        // Web 目标缺 domain → 契约校验失败，存储与快照都不动
        let mut bad_target = record("acc-a", "github.com", None);
        bad_target.target.domain = None;
        let svc = service(store.clone(), vec![bad_target], BASE, policy());
        assert!(matches!(
            svc.capture(&chromium()),
            Err(StateError::Contract(_))
        ));
        assert!(store.load_sessions().expect("读取失败").is_empty());
        assert!(svc.list_snapshots().expect("列快照失败").is_empty());

        // 到期时间非法 → InvalidEntry
        let svc = service(
            store.clone(),
            vec![record("acc-a", "github.com", Some("2026-13-01T00:00:00Z"))],
            BASE,
            policy(),
        );
        assert_eq!(
            svc.capture(&chromium()),
            Err(StateError::InvalidEntry {
                field: "expires_at",
                value: "2026-13-01T00:00:00Z".to_string()
            })
        );
        assert!(svc.list_snapshots().expect("列快照失败").is_empty());
    }

    #[test]
    fn unknown_snapshot_reports_not_found() {
        let svc = service(InMemoryStateStore::new(), Vec::new(), BASE, policy());
        assert_eq!(
            svc.snapshot("nope"),
            Err(StateError::SnapshotNotFound("nope".to_string()))
        );
        assert_eq!(
            svc.rollback("nope"),
            Err(StateError::SnapshotNotFound("nope".to_string()))
        );
    }

    #[test]
    fn snapshot_json_roundtrip_keeps_fields_snake_case() {
        let (store, _) = seeded();
        let svc = service(store, vec![record("acc-a", "github.com", None)], BASE, policy());
        let report = svc.capture(&chromium()).expect("capture 失败");
        let snapshot = svc.snapshot(&report.snapshot.id).expect("读快照失败");

        let json = serde_json::to_string(&snapshot).expect("序列化失败");
        assert!(json.contains("\"created_at\""));
        assert!(json.contains("\"account_ref\""));
        assert!(json.contains("\"expires_at\""));
        assert!(json.contains("\"source\""));

        let restored: Snapshot = serde_json::from_str(&json).expect("反序列化失败");
        assert_eq!(restored, snapshot);
    }
}
