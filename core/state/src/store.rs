//! 存储注入点：`StateStore` 由宿主实现（桌面 = 本地加密库，移动 = 平台存储，
//! 单测 = [`InMemoryStateStore`]）。ab-state 自身不落盘、不碰文件系统。

use std::sync::{Arc, Mutex};

use crate::error::StateError;
use crate::model::{SessionEntry, Snapshot, SnapshotMeta};

/// 会话与快照的持久化接口。
///
/// 实现须 `Send + Sync`：服务以 `&self` 调用，同步原语由实现自持（仓库无全局单例）。
pub trait StateStore: Send + Sync {
    /// 读取全部会话；顺序由 [`StateStore::replace_sessions`] 的写入方保证（身份规范化序）。
    fn load_sessions(&self) -> Result<Vec<SessionEntry>, StateError>;

    /// 用给定集合整体替换全部会话。
    fn replace_sessions(&self, entries: Vec<SessionEntry>) -> Result<(), StateError>;

    /// 列出快照元数据；顺序不作保证（由调用方排序）。
    fn list_snapshots(&self) -> Result<Vec<SnapshotMeta>, StateError>;

    /// 保存快照；同 id 覆盖。
    fn save_snapshot(&self, snapshot: Snapshot) -> Result<(), StateError>;

    /// 读取快照正文，不存在返回 `Ok(None)`。
    fn load_snapshot(&self, id: &str) -> Result<Option<Snapshot>, StateError>;
}

/// 内存实现：单测与宿主自检用，不做任何持久化。
///
/// `Clone` 为句柄语义：克隆共享同一份数据（宿主与 service 可各持一个句柄）。
#[derive(Debug, Default, Clone)]
pub struct InMemoryStateStore {
    inner: Arc<Mutex<Inner>>,
}

#[derive(Debug, Default)]
struct Inner {
    sessions: Vec<SessionEntry>,
    snapshots: Vec<Snapshot>,
}

impl InMemoryStateStore {
    /// 空存储。
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner::default())),
        }
    }

    fn with<T>(&self, access: impl FnOnce(&mut Inner) -> T) -> Result<T, StateError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| StateError::Store("内存存储互斥锁中毒".to_string()))?;
        Ok(access(&mut guard))
    }
}

impl StateStore for InMemoryStateStore {
    fn load_sessions(&self) -> Result<Vec<SessionEntry>, StateError> {
        self.with(|inner| inner.sessions.clone())
    }

    fn replace_sessions(&self, entries: Vec<SessionEntry>) -> Result<(), StateError> {
        self.with(|inner| {
            inner.sessions = entries;
        })
    }

    fn list_snapshots(&self) -> Result<Vec<SnapshotMeta>, StateError> {
        self.with(|inner| inner.snapshots.iter().map(|snapshot| snapshot.meta()).collect())
    }

    fn save_snapshot(&self, snapshot: Snapshot) -> Result<(), StateError> {
        self.with(|inner| {
            let existing = inner
                .snapshots
                .iter()
                .position(|saved| saved.id == snapshot.id);
            match existing {
                Some(index) => inner.snapshots[index] = snapshot,
                None => inner.snapshots.push(snapshot),
            }
        })
    }

    fn load_snapshot(&self, id: &str) -> Result<Option<Snapshot>, StateError> {
        self.with(|inner| {
            inner
                .snapshots
                .iter()
                .find(|snapshot| snapshot.id == id)
                .cloned()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::EndpointKind;
    use crate::test_support::*;

    fn snap(id: &str, account: &str) -> Snapshot {
        Snapshot {
            id: id.to_string(),
            created_at: BASE,
            entries: vec![entry(
                EndpointKind::Browser,
                "chromium",
                account,
                "github.com",
                None,
            )],
        }
    }

    #[test]
    fn replaces_reads_and_lists() {
        let store = InMemoryStateStore::new();
        assert_eq!(store.load_sessions(), Ok(Vec::new()));

        let entries = vec![entry(
            EndpointKind::Mobile,
            "ios",
            "acc-a",
            "github.com",
            None,
        )];
        store.replace_sessions(entries.clone()).expect("写入失败");
        assert_eq!(store.load_sessions(), Ok(entries));

        store.save_snapshot(snap("snap-1", "acc-a")).expect("存快照失败");
        let metas = store.list_snapshots().expect("列快照失败");
        assert_eq!(metas.len(), 1);
        assert_eq!(metas[0].id, "snap-1");
        assert_eq!(metas[0].entry_count, 1);
        assert_eq!(metas[0].created_at, BASE);

        assert_eq!(
            store.load_snapshot("snap-1").expect("读快照失败"),
            Some(snap("snap-1", "acc-a"))
        );
        assert_eq!(store.load_snapshot("nope").expect("读快照失败"), None);
    }

    #[test]
    fn saving_same_id_overwrites_and_clone_shares_state() {
        let store = InMemoryStateStore::new();
        let handle = store.clone();
        store.save_snapshot(snap("snap-1", "acc-a")).expect("存快照失败");
        handle
            .save_snapshot(snap("snap-1", "acc-b"))
            .expect("覆盖失败");

        let metas = store.list_snapshots().expect("列快照失败");
        assert_eq!(metas.len(), 1);
        let saved = store
            .load_snapshot("snap-1")
            .expect("读快照失败")
            .expect("快照丢失");
        assert_eq!(saved.entries[0].record.account_ref, "acc-b");
        assert!(handle.load_snapshot("snap-1").is_ok());
    }
}
