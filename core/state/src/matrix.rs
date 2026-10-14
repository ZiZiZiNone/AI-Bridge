//! 会话矩阵：账号 × 端 × 来源 × 到期 的筛选与排序（功能点 A2.1 / A2.3 / B1.3）。
//!
//! 行内**不含 payload**：清单只暴露展示所需字段（账号 / 端 / 来源 / 目标 / 到期 / 健康），
//! 会话正文不经过 UI 层。

use ab_contract::{SessionKind, Source, SourceType, Target, TargetKind};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

use crate::error::StateError;
use crate::health::{grade, HealthLevel};
use crate::model::{
    compare_expiry_asc, compare_expiry_desc, source_rank, Endpoint, EndpointKind, SessionEntry,
};
use crate::{Clock, SessionSource, StateService, StateStore};

/// 到期轴筛选。边界口径：`remaining = 到期时刻 - 现在`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExpiryFilter {
    /// 不过滤。
    #[default]
    Any,
    /// 已到期：`remaining <= 0`。
    Expired,
    /// 在窗口内到期：`0 < remaining <= within_secs`（含边界）。
    Expiring { within_secs: i64 },
    /// 无到期时间。
    WithoutExpiry,
}

/// 主排序键。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortKey {
    /// 账号引用。
    AccountRef,
    /// 端（kind, id）。
    Endpoint,
    /// 来源（来源类型序, 来源 id）。
    Source,
    /// 到期（升序 = 最紧急在前）。
    ExpiresAt,
}

/// 排序设置；同键条目保持清单顺序（存储已按身份规范化）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatrixSort {
    pub key: SortKey,
    /// `true` 升序。到期轴例外：无到期时间恒在最后，不受方向影响。
    pub ascending: bool,
}

impl Default for MatrixSort {
    fn default() -> Self {
        MatrixSort {
            key: SortKey::ExpiresAt,
            ascending: true,
        }
    }
}

/// 矩阵筛选条件：账号 / 端 / 来源 / 到期，缺省即不过滤。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MatrixFilters {
    /// 账号引用，精确匹配。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_ref: Option<String>,
    /// 端 id，精确匹配。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint_id: Option<String>,
    /// 端类型。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint_kind: Option<EndpointKind>,
    /// 来源类型（契约 `source.type`）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_type: Option<SourceType>,
    /// 来源 id（哪个扩展 / 转换器 / 扩散事件）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    /// 到期轴。
    pub expiry: ExpiryFilter,
    /// 排序。
    pub sort: MatrixSort,
}

impl MatrixFilters {
    fn matches(&self, entry: &SessionEntry, remaining_secs: Option<i64>) -> bool {
        if let Some(want) = &self.account_ref {
            if &entry.record.account_ref != want {
                return false;
            }
        }
        if let Some(want) = &self.endpoint_id {
            if &entry.endpoint.id != want {
                return false;
            }
        }
        if let Some(want) = self.endpoint_kind {
            if entry.endpoint.kind != want {
                return false;
            }
        }
        if let Some(want) = self.source_type {
            if entry.record.source.source_type != want {
                return false;
            }
        }
        if let Some(want) = &self.source_id {
            if &entry.record.source.id != want {
                return false;
            }
        }
        match self.expiry {
            ExpiryFilter::Any => true,
            ExpiryFilter::Expired => matches!(remaining_secs, Some(remaining) if remaining <= 0),
            ExpiryFilter::Expiring { within_secs } => {
                matches!(remaining_secs, Some(remaining) if remaining > 0 && remaining <= within_secs)
            }
            ExpiryFilter::WithoutExpiry => remaining_secs.is_none(),
        }
    }
}

/// 矩阵行：清单总览所需字段（无 payload）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatrixRow {
    pub account_ref: String,
    pub endpoint: Endpoint,
    pub source: Source,
    pub target: Target,
    pub kind: SessionKind,
    /// 契约原始到期时间（展示用）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    /// 到期倒计时（秒）；无到期时间为 `None`，已过期为负数或 0。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remaining_secs: Option<i64>,
    /// 健康分级。
    pub health: HealthLevel,
}

impl<S: StateStore, C: Clock, R: SessionSource> StateService<S, C, R> {
    /// 会话矩阵：按 [`MatrixFilters`] 筛选并排序。
    pub fn matrix(&self, filters: &MatrixFilters) -> Result<Vec<MatrixRow>, StateError> {
        let now = self.clock.now_unix_secs();
        let policy = self.health_policy;
        let entries = self.store.load_sessions()?;
        let mut rows: Vec<MatrixRow> = Vec::with_capacity(entries.len());

        for entry in entries {
            let remaining_secs = entry.expires_at_secs()?.map(|secs| secs - now);
            if !filters.matches(&entry, remaining_secs) {
                continue;
            }
            rows.push(MatrixRow {
                account_ref: entry.record.account_ref.clone(),
                endpoint: entry.endpoint.clone(),
                source: entry.record.source.clone(),
                target: entry.record.target.clone(),
                kind: entry.record.kind,
                expires_at: entry.record.expires_at.clone(),
                remaining_secs,
                health: grade(remaining_secs, &policy),
            });
        }

        rows.sort_by(|a, b| compare_rows(&filters.sort, a, b));
        Ok(rows)
    }

    /// 按目标查会话（含 payload）：消费方为 ab-broker 代持（ab-mcp `acquire_session`）
    /// 与宿主注入；同站多账号并存时返回全部候选，由调用方消歧。
    pub fn sessions_for_target(&self, target: &Target) -> Result<Vec<SessionEntry>, StateError> {
        let entries = self.store.load_sessions()?;
        Ok(entries
            .into_iter()
            .filter(|entry| target_matches(&entry.record.target, target))
            .collect())
    }
}

/// 目标匹配：kind 相同后按对应键比对（web=domain、app=app_id；大小写与首尾点不敏感）。
fn target_matches(stored: &Target, want: &Target) -> bool {
    if stored.kind != want.kind {
        return false;
    }
    let norm = |value: Option<&str>| -> String {
        value
            .unwrap_or_default()
            .trim()
            .trim_start_matches('.')
            .trim_end_matches('.')
            .to_ascii_lowercase()
    };
    match want.kind {
        TargetKind::Web => {
            let (a, b) = (norm(stored.domain.as_deref()), norm(want.domain.as_deref()));
            !a.is_empty() && a == b
        }
        TargetKind::App => {
            let (a, b) = (norm(stored.app_id.as_deref()), norm(want.app_id.as_deref()));
            !a.is_empty() && a == b
        }
        TargetKind::Container => true,
    }
}

fn compare_rows(sort: &MatrixSort, a: &MatrixRow, b: &MatrixRow) -> Ordering {
    match sort.key {
        SortKey::AccountRef => directed(a.account_ref.cmp(&b.account_ref), sort.ascending),
        SortKey::Endpoint => directed(a.endpoint.cmp(&b.endpoint), sort.ascending),
        SortKey::Source => directed(source_cmp(&a.source, &b.source), sort.ascending),
        SortKey::ExpiresAt => {
            if sort.ascending {
                compare_expiry_asc(a.remaining_secs, b.remaining_secs)
            } else {
                compare_expiry_desc(a.remaining_secs, b.remaining_secs)
            }
        }
    }
}

fn directed(ordering: Ordering, ascending: bool) -> Ordering {
    if ascending {
        ordering
    } else {
        ordering.reverse()
    }
}

fn source_cmp(a: &Source, b: &Source) -> Ordering {
    source_rank(a.source_type)
        .cmp(&source_rank(b.source_type))
        .then_with(|| a.id.cmp(&b.id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::HealthPolicy;
    use crate::store::{InMemoryStateStore, StateStore};
    use crate::test_support::*;

    fn policy() -> HealthPolicy {
        HealthPolicy {
            warn_within_secs: 100,
        }
    }

    /// 清单顺序（存储身份规范化序）：acc-a/chromium → acc-a/ios → acc-b/firefox → acc-c/desktop
    fn fixture() -> TestService {
        let store = store_with(vec![
            entry_full(
                EndpointKind::Browser,
                "chromium",
                "acc-a",
                "github.com",
                Some(T_PLUS_100),
                SourceType::Extension,
                "chromium",
            ),
            entry_full(
                EndpointKind::Browser,
                "firefox",
                "acc-b",
                "gitlab.com",
                Some(T_MINUS_1),
                SourceType::Extension,
                "firefox",
            ),
            entry_full(
                EndpointKind::Mobile,
                "ios",
                "acc-a",
                "github.com",
                None,
                SourceType::Mobile,
                "ios",
            ),
            entry_full(
                EndpointKind::Desktop,
                "desktop",
                "acc-c",
                "corp.example",
                Some(T_PLUS_DAY),
                SourceType::Sync,
                "peer-1",
            ),
        ]);
        service(store, Vec::new(), BASE, policy())
    }

    #[test]
    fn default_filters_return_all_rows_sorted_by_urgency() {
        let svc = fixture();
        let rows = svc.matrix(&MatrixFilters::default()).expect("matrix 失败");

        let accounts: Vec<&str> = rows.iter().map(|row| row.account_ref.as_str()).collect();
        assert_eq!(accounts, ["acc-b", "acc-a", "acc-c", "acc-a"]);

        assert_eq!(rows[0].health, HealthLevel::Expired);
        assert_eq!(rows[0].remaining_secs, Some(-1));
        assert_eq!(rows[1].health, HealthLevel::ExpiringSoon);
        assert_eq!(rows[1].remaining_secs, Some(100));
        assert_eq!(rows[2].health, HealthLevel::Active);
        assert_eq!(rows[2].remaining_secs, Some(86_400));
        assert_eq!(rows[3].health, HealthLevel::Active);
        assert_eq!(rows[3].remaining_secs, None);
    }

    #[test]
    fn filters_match_each_axis_and_combine() {
        let svc = fixture();

        let by_account = MatrixFilters {
            account_ref: Some("acc-a".to_string()),
            ..MatrixFilters::default()
        };
        let rows = svc.matrix(&by_account).expect("matrix 失败");
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|row| row.account_ref == "acc-a"));

        let by_endpoint = MatrixFilters {
            endpoint_id: Some("firefox".to_string()),
            ..MatrixFilters::default()
        };
        assert_eq!(svc.matrix(&by_endpoint).expect("matrix 失败").len(), 1);

        let by_kind = MatrixFilters {
            endpoint_kind: Some(EndpointKind::Browser),
            ..MatrixFilters::default()
        };
        assert_eq!(svc.matrix(&by_kind).expect("matrix 失败").len(), 2);

        let by_source_type = MatrixFilters {
            source_type: Some(SourceType::Extension),
            ..MatrixFilters::default()
        };
        assert_eq!(svc.matrix(&by_source_type).expect("matrix 失败").len(), 2);

        let by_source_id = MatrixFilters {
            source_id: Some("peer-1".to_string()),
            ..MatrixFilters::default()
        };
        assert_eq!(svc.matrix(&by_source_id).expect("matrix 失败").len(), 1);

        let combined = MatrixFilters {
            account_ref: Some("acc-a".to_string()),
            endpoint_kind: Some(EndpointKind::Mobile),
            source_id: Some("ios".to_string()),
            expiry: ExpiryFilter::WithoutExpiry,
            ..MatrixFilters::default()
        };
        let rows = svc.matrix(&combined).expect("matrix 失败");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].endpoint.id, "ios");
        assert_eq!(rows[0].source.source_type, SourceType::Mobile);
        assert_eq!(rows[0].target.domain.as_deref(), Some("github.com"));
        assert_eq!(rows[0].kind, SessionKind::Cookie);

        let impossible = MatrixFilters {
            account_ref: Some("acc-a".to_string()),
            endpoint_id: Some("firefox".to_string()),
            ..MatrixFilters::default()
        };
        assert!(svc.matrix(&impossible).expect("matrix 失败").is_empty());
    }

    #[test]
    fn expiry_filter_boundaries() {
        let svc = fixture();
        let count = |expiry: ExpiryFilter| {
            svc.matrix(&MatrixFilters {
                expiry,
                ..MatrixFilters::default()
            })
            .expect("matrix 失败")
            .len()
        };

        assert_eq!(count(ExpiryFilter::Any), 4);
        assert_eq!(count(ExpiryFilter::Expired), 1);
        assert_eq!(count(ExpiryFilter::Expiring { within_secs: 100 }), 1);
        assert_eq!(count(ExpiryFilter::Expiring { within_secs: 99 }), 0);
        assert_eq!(count(ExpiryFilter::WithoutExpiry), 1);
    }

    #[test]
    fn sorts_by_account_endpoint_source_and_expiry() {
        let svc = fixture();
        let rows_for = |key: SortKey, ascending: bool| {
            svc.matrix(&MatrixFilters {
                sort: MatrixSort { key, ascending },
                ..MatrixFilters::default()
            })
            .expect("matrix 失败")
        };

        let rows = rows_for(SortKey::AccountRef, true);
        let accounts: Vec<&str> = rows.iter().map(|row| row.account_ref.as_str()).collect();
        assert_eq!(accounts, ["acc-a", "acc-a", "acc-b", "acc-c"]);
        assert_eq!(rows[0].endpoint.id, "chromium");
        assert_eq!(rows[1].endpoint.id, "ios");

        let rows = rows_for(SortKey::AccountRef, false);
        let accounts: Vec<&str> = rows.iter().map(|row| row.account_ref.as_str()).collect();
        assert_eq!(accounts, ["acc-c", "acc-b", "acc-a", "acc-a"]);
        assert_eq!(rows[2].endpoint.id, "chromium");
        assert_eq!(rows[3].endpoint.id, "ios");

        let rows = rows_for(SortKey::Endpoint, true);
        let endpoints: Vec<&str> = rows.iter().map(|row| row.endpoint.id.as_str()).collect();
        assert_eq!(endpoints, ["chromium", "firefox", "desktop", "ios"]);

        let rows = rows_for(SortKey::Source, true);
        let sources: Vec<&str> = rows.iter().map(|row| row.source.id.as_str()).collect();
        assert_eq!(sources, ["chromium", "firefox", "ios", "peer-1"]);

        let rows = rows_for(SortKey::ExpiresAt, false);
        let accounts: Vec<&str> = rows.iter().map(|row| row.account_ref.as_str()).collect();
        assert_eq!(accounts, ["acc-c", "acc-a", "acc-b", "acc-a"]);
        assert_eq!(rows[3].endpoint.id, "ios");
        assert_eq!(rows[3].remaining_secs, None);
    }

    #[test]
    fn filters_deserialize_with_defaults() {
        let filters: MatrixFilters = serde_json::from_str("{}").expect("反序列化失败");
        assert_eq!(filters, MatrixFilters::default());
        assert_eq!(filters.expiry, ExpiryFilter::Any);
        assert_eq!(filters.sort.key, SortKey::ExpiresAt);
        assert!(filters.sort.ascending);
    }

    #[test]
    fn matrix_rejects_unparseable_expiry_in_store() {
        // 绕过写入校验，模拟宿主侧旧数据入库后的坏值
        let store = InMemoryStateStore::new();
        let bad = SessionEntry::new(
            Endpoint::new(EndpointKind::Browser, "chromium"),
            record("acc-a", "github.com", Some("2026-13-01T00:00:00Z")),
        );
        store.replace_sessions(vec![bad]).expect("写入失败");

        let svc = service(store, Vec::new(), BASE, policy());
        assert_eq!(
            svc.matrix(&MatrixFilters::default()),
            Err(StateError::InvalidEntry {
                field: "expires_at",
                value: "2026-13-01T00:00:00Z".to_string()
            })
        );
    }
}
