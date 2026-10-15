//! 状态层数据模型：端、会话条目、快照与捕获 / 回滚报告。
//!
//! 会话本体不做二次建模：一律复用契约层 `ab_contract::SessionRecord`（契约唯一
//! 事实源），状态层只补「这份会话现在待在哪个端」这一矩阵维度。

use ab_contract::{SessionKind, SessionRecord, SourceType, Target, TargetKind};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

use crate::error::StateError;

/// 端类型（会话当前所在宿主）。与契约 `source.type`（来源）是两个独立的轴。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointKind {
    /// 浏览器扩展（具体浏览器见 id）。**声明序即「端」轴排序序**：浏览器 → 桌面 → 移动（主通道优先）。
    Browser,
    /// 桌面程序。
    Desktop,
    /// 移动端（iOS / Android）。
    Mobile,
}

/// 端标识：矩阵「端」轴。派生 `Ord` 的顺序为（kind, id），规范化与排序同用。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Endpoint {
    pub kind: EndpointKind,
    pub id: String,
}

impl Endpoint {
    /// 构造端标识；`id` 语义由宿主约定（如 `chromium` / `firefox` / `ios`）。
    pub fn new(kind: EndpointKind, id: impl Into<String>) -> Self {
        Self { kind, id: id.into() }
    }
}

/// 会话条目 = 契约会话本体 + 所在端。存储与快照的最小单元。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionEntry {
    pub endpoint: Endpoint,
    pub record: SessionRecord,
}

impl SessionEntry {
    /// 组装条目。
    pub fn new(endpoint: Endpoint, record: SessionRecord) -> Self {
        Self { endpoint, record }
    }

    /// 写入前校验：契约结构校验（`SessionRecord::validate`）+ 状态层前置
    /// （账号引用非空、端 id 非空、到期时间为合法 RFC3339）。
    pub fn validate(&self) -> Result<(), StateError> {
        if self.record.account_ref.is_empty() {
            return Err(StateError::InvalidEntry {
                field: "account_ref",
                value: String::new(),
            });
        }
        if self.endpoint.id.is_empty() {
            return Err(StateError::InvalidEntry {
                field: "endpoint.id",
                value: String::new(),
            });
        }
        self.record.validate()?;
        if let Some(expires_at) = &self.record.expires_at {
            if ab_contract::time::parse_rfc3339_secs(expires_at).is_none() {
                return Err(StateError::InvalidEntry {
                    field: "expires_at",
                    value: expires_at.clone(),
                });
            }
        }
        Ok(())
    }

    /// 到期时刻（Unix 纪元秒）；无到期时间返回 `None`。
    pub fn expires_at_secs(&self) -> Result<Option<i64>, StateError> {
        match &self.record.expires_at {
            None => Ok(None),
            Some(expires_at) => match ab_contract::time::parse_rfc3339_secs(expires_at) {
                Some(secs) => Ok(Some(secs)),
                None => Err(StateError::InvalidEntry {
                    field: "expires_at",
                    value: expires_at.clone(),
                }),
            },
        }
    }
}

/// 覆盖前快照：整份条目集合 + 生成时刻，回滚按原样写回。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: String,
    /// 生成时刻（Unix 纪元秒，UTC）。
    pub created_at: i64,
    pub entries: Vec<SessionEntry>,
}

impl Snapshot {
    /// 轻量元数据（列表用，不带条目正文）。
    pub fn meta(&self) -> SnapshotMeta {
        SnapshotMeta {
            id: self.id.clone(),
            created_at: self.created_at,
            entry_count: self.entries.len(),
        }
    }
}

/// 快照元数据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotMeta {
    pub id: String,
    /// 生成时刻（Unix 纪元秒，UTC）。
    pub created_at: i64,
    pub entry_count: usize,
}

/// 捕获报告：覆盖前快照句柄 + 条目统计（供宿主与审计使用）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureReport {
    pub snapshot: SnapshotMeta,
    /// 本次从端读入并写入的条目数。
    pub captured_entries: usize,
    /// 被覆盖（同一端）的旧条目数。
    pub replaced_entries: usize,
}

/// 回滚报告。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RollbackReport {
    pub snapshot_id: String,
    /// 从快照恢复的条目数。
    pub restored_entries: usize,
    /// 回滚前被替换掉的条目数。
    pub replaced_entries: usize,
}

/* ---------------- 规范化排序（回滚一致性依赖它） ---------------- */

/// 来源类型排序序（契约枚举未实现 `Ord`，此处给出状态层固定序）。
pub(crate) fn source_rank(source_type: SourceType) -> u8 {
    match source_type {
        SourceType::Extension => 0,
        SourceType::Converter => 1,
        SourceType::Mobile => 2,
        SourceType::Manual => 3,
        SourceType::Sync => 4,
    }
}

/// 会话类型排序序。
pub(crate) fn kind_rank(kind: SessionKind) -> u8 {
    match kind {
        SessionKind::Cookie => 0,
        SessionKind::Token => 1,
        SessionKind::Credential => 2,
    }
}

/// 目标类型排序序。
pub(crate) fn target_rank(kind: TargetKind) -> u8 {
    match kind {
        TargetKind::Web => 0,
        TargetKind::App => 1,
        TargetKind::Container => 2,
    }
}

fn target_cmp(a: &Target, b: &Target) -> Ordering {
    target_rank(a.kind)
        .cmp(&target_rank(b.kind))
        .then_with(|| a.domain.cmp(&b.domain))
        .then_with(|| a.app_id.cmp(&b.app_id))
}

/// 条目身份序：账号 → 端 → 来源 → 会话类型 → 目标 → 到期。
/// 同键条目保持输入顺序（`sort_by` 稳定）。
pub(crate) fn identity_cmp(a: &SessionEntry, b: &SessionEntry) -> Ordering {
    a.record
        .account_ref
        .cmp(&b.record.account_ref)
        .then_with(|| a.endpoint.cmp(&b.endpoint))
        .then_with(|| {
            source_rank(a.record.source.source_type).cmp(&source_rank(b.record.source.source_type))
        })
        .then_with(|| a.record.source.id.cmp(&b.record.source.id))
        .then_with(|| kind_rank(a.record.kind).cmp(&kind_rank(b.record.kind)))
        .then_with(|| target_cmp(&a.record.target, &b.record.target))
        .then_with(|| a.record.expires_at.cmp(&b.record.expires_at))
}

/// 写入存储与快照前的规范化：保证同一份集合在任何输入顺序下字节级一致。
pub(crate) fn canonicalize(mut entries: Vec<SessionEntry>) -> Vec<SessionEntry> {
    entries.sort_by(identity_cmp);
    entries
}

/* ---------------- 到期排序（矩阵与健康共用，无到期恒在最后） ---------------- */

/// 到期升序：最紧急在前，无到期时间殿后。
pub(crate) fn compare_expiry_asc(a: Option<i64>, b: Option<i64>) -> Ordering {
    match (a, b) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// 到期降序：无到期时间仍在最后（不受方向影响）。
pub(crate) fn compare_expiry_desc(a: Option<i64>, b: Option<i64>) -> Ordering {
    match (a, b) {
        (Some(x), Some(y)) => y.cmp(&x),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;

    #[test]
    fn canonicalize_is_order_insensitive() {
        let chromium = entry(
            EndpointKind::Browser,
            "chromium",
            "acc-a",
            "github.com",
            Some(T_PLUS_DAY),
        );
        let firefox = entry(
            EndpointKind::Browser,
            "firefox",
            "acc-a",
            "github.com",
            Some(T_PLUS_DAY),
        );
        let ios = entry(EndpointKind::Mobile, "ios", "acc-a", "github.com", None);

        let forward = canonicalize(vec![chromium.clone(), firefox.clone(), ios.clone()]);
        let backward = canonicalize(vec![ios, firefox, chromium]);

        assert_eq!(forward, backward);
        assert_eq!(forward[0].endpoint.id, "chromium");
        assert_eq!(forward[1].endpoint.id, "firefox");
        assert_eq!(forward[2].endpoint.id, "ios");
    }

    #[test]
    fn validate_rejects_empty_account_and_endpoint() {
        let mut empty_account = entry(EndpointKind::Browser, "chromium", "acc-a", "github.com", None);
        empty_account.record.account_ref.clear();
        assert_eq!(
            empty_account.validate(),
            Err(StateError::InvalidEntry {
                field: "account_ref",
                value: String::new()
            })
        );

        let mut empty_endpoint = entry(EndpointKind::Browser, "chromium", "acc-a", "github.com", None);
        empty_endpoint.endpoint.id.clear();
        assert_eq!(
            empty_endpoint.validate(),
            Err(StateError::InvalidEntry {
                field: "endpoint.id",
                value: String::new()
            })
        );
    }

    #[test]
    fn validate_rejects_bad_expiry_and_contract_violation() {
        let bad_expiry = entry(
            EndpointKind::Browser,
            "chromium",
            "acc-a",
            "github.com",
            Some("2026-02-29T00:00:00Z"),
        );
        assert_eq!(
            bad_expiry.validate(),
            Err(StateError::InvalidEntry {
                field: "expires_at",
                value: "2026-02-29T00:00:00Z".to_string()
            })
        );

        // Web 目标缺 domain 由契约层判失败，状态层直接转发该错误。
        let mut bad_target = entry(EndpointKind::Browser, "chromium", "acc-a", "github.com", None);
        bad_target.record.target.domain = None;
        assert!(matches!(
            bad_target.validate(),
            Err(StateError::Contract(_))
        ));
    }

    #[test]
    fn expires_at_secs_reports_missing_and_parses_valid() {
        let no_expiry = entry(EndpointKind::Browser, "chromium", "acc-a", "github.com", None);
        assert_eq!(no_expiry.expires_at_secs(), Ok(None));

        let with_expiry = entry(
            EndpointKind::Browser,
            "chromium",
            "acc-a",
            "github.com",
            Some(T_PLUS_DAY),
        );
        assert_eq!(with_expiry.expires_at_secs(), Ok(Some(T_PLUS_DAY_SECS)));
    }
}
