//! 会话健康：到期倒计时与告警分级（活跃 / 将过期 / 已过期，功能点 A3.1）。

use serde::{Deserialize, Serialize};

use crate::error::StateError;
use crate::model::{compare_expiry_asc, Endpoint};
use crate::{Clock, SessionSource, StateService, StateStore};

/// 默认告警阈值：到期前 24 小时进入「将过期」。
pub const DEFAULT_WARN_WITHIN_SECS: i64 = 24 * 60 * 60;

/// 告警策略。UI 只呈现结果，阈值一律在此配置（UI 不做业务判断）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthPolicy {
    /// 提前告警窗口（秒）：`0 < remaining <= warn_within_secs` 判为将过期。
    pub warn_within_secs: i64,
}

impl Default for HealthPolicy {
    fn default() -> Self {
        HealthPolicy {
            warn_within_secs: DEFAULT_WARN_WITHIN_SECS,
        }
    }
}

/// 健康分级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthLevel {
    /// 活跃：剩余时间大于告警窗口。
    Active,
    /// 将过期：`0 < 剩余 <= 告警窗口`。
    ExpiringSoon,
    /// 已过期：`剩余 <= 0`。
    Expired,
}

/// 分级唯一实现（矩阵行与健康报告共用同一份规则）。
///
/// 边界口径：无到期时间视为活跃（长期有效）；`剩余 == 0` 记已过期；
/// `剩余 == warn_within_secs` 记将过期（含边界）。
pub(crate) fn grade(remaining_secs: Option<i64>, policy: &HealthPolicy) -> HealthLevel {
    match remaining_secs {
        None => HealthLevel::Active,
        Some(remaining) if remaining <= 0 => HealthLevel::Expired,
        Some(remaining) if remaining <= policy.warn_within_secs => HealthLevel::ExpiringSoon,
        Some(_) => HealthLevel::Active,
    }
}

/// 健康条目：通知所需字段（账号 / 端 / 级别 / 倒计时），不含 payload。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthItem {
    pub account_ref: String,
    pub endpoint: Endpoint,
    pub level: HealthLevel,
    /// 到期倒计时（秒）；无到期时间为 `None`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remaining_secs: Option<i64>,
}

/// 健康报告：分级计数 + 按紧急度排序的条目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthReport {
    /// 计算时刻（Unix 纪元秒，UTC）。
    pub now_unix_secs: i64,
    /// 本次使用的告警窗口。
    pub warn_within_secs: i64,
    pub active: usize,
    pub expiring_soon: usize,
    pub expired: usize,
    /// 最紧急在前：到期时刻升序，无到期时间殿后。
    pub items: Vec<HealthItem>,
}

impl<S: StateStore, C: Clock, R: SessionSource> StateService<S, C, R> {
    /// 会话健康：逐条算到期倒计时并分级，汇总计数。
    pub fn health(&self) -> Result<HealthReport, StateError> {
        let now = self.clock.now_unix_secs();
        let policy = self.health_policy;
        let entries = self.store.load_sessions()?;

        let mut items: Vec<HealthItem> = Vec::with_capacity(entries.len());
        for entry in entries {
            let remaining_secs = entry.expires_at_secs()?.map(|secs| secs - now);
            items.push(HealthItem {
                account_ref: entry.record.account_ref.clone(),
                endpoint: entry.endpoint.clone(),
                level: grade(remaining_secs, &policy),
                remaining_secs,
            });
        }
        items.sort_by(|a, b| {
            compare_expiry_asc(a.remaining_secs, b.remaining_secs)
                .then_with(|| a.account_ref.cmp(&b.account_ref))
                .then_with(|| a.endpoint.cmp(&b.endpoint))
        });

        let mut active = 0;
        let mut expiring_soon = 0;
        let mut expired = 0;
        for item in &items {
            match item.level {
                HealthLevel::Active => active += 1,
                HealthLevel::ExpiringSoon => expiring_soon += 1,
                HealthLevel::Expired => expired += 1,
            }
        }

        Ok(HealthReport {
            now_unix_secs: now,
            warn_within_secs: policy.warn_within_secs,
            active,
            expiring_soon,
            expired,
            items,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::EndpointKind;
    use crate::test_support::*;

    fn policy() -> HealthPolicy {
        HealthPolicy {
            warn_within_secs: 100,
        }
    }

    #[test]
    fn default_policy_warns_one_day_ahead() {
        assert_eq!(DEFAULT_WARN_WITHIN_SECS, 86_400);
        assert_eq!(HealthPolicy::default().warn_within_secs, 86_400);
    }

    #[test]
    fn grades_every_boundary() {
        let store = store_with(vec![
            entry(
                EndpointKind::Browser,
                "chromium",
                "acc-active",
                "a.example",
                Some(T_PLUS_101),
            ),
            entry(
                EndpointKind::Browser,
                "firefox",
                "acc-warn-exact",
                "b.example",
                Some(T_PLUS_100),
            ),
            entry(
                EndpointKind::Browser,
                "edge",
                "acc-warn-near",
                "c.example",
                Some(T_PLUS_1),
            ),
            entry(
                EndpointKind::Desktop,
                "desktop",
                "acc-zero",
                "d.example",
                Some(T_BASE),
            ),
            entry(
                EndpointKind::Mobile,
                "ios",
                "acc-expired",
                "e.example",
                Some(T_MINUS_1),
            ),
            entry(
                EndpointKind::Mobile,
                "android",
                "acc-forever",
                "f.example",
                None,
            ),
        ]);
        let svc = service(store, Vec::new(), BASE, policy());
        let report = svc.health().expect("health 失败");

        assert_eq!(report.now_unix_secs, BASE);
        assert_eq!(report.warn_within_secs, 100);
        assert_eq!(report.active, 2);
        assert_eq!(report.expiring_soon, 2);
        assert_eq!(report.expired, 2);
        assert_eq!(report.items.len(), 6);

        let level_of = |account: &str| -> HealthLevel {
            report
                .items
                .iter()
                .find(|item| item.account_ref == account)
                .expect("账号缺失")
                .level
        };
        assert_eq!(level_of("acc-active"), HealthLevel::Active);
        assert_eq!(level_of("acc-warn-exact"), HealthLevel::ExpiringSoon);
        assert_eq!(level_of("acc-warn-near"), HealthLevel::ExpiringSoon);
        assert_eq!(level_of("acc-zero"), HealthLevel::Expired);
        assert_eq!(level_of("acc-expired"), HealthLevel::Expired);
        assert_eq!(level_of("acc-forever"), HealthLevel::Active);

        // 紧急度排序：-1 → 0 → 1 → 100 → 101 → 无到期
        assert_eq!(report.items[0].account_ref, "acc-expired");
        assert_eq!(report.items[1].account_ref, "acc-zero");
        assert_eq!(report.items[2].remaining_secs, Some(1));
        assert_eq!(report.items[3].remaining_secs, Some(100));
        assert_eq!(report.items[4].remaining_secs, Some(101));
        assert_eq!(report.items[5].account_ref, "acc-forever");
        assert_eq!(report.items[5].remaining_secs, None);
    }

    #[test]
    fn empty_store_reports_empty_health() {
        let svc = service(crate::store::InMemoryStateStore::new(), Vec::new(), BASE, policy());
        let report = svc.health().expect("health 失败");
        assert_eq!(report.active, 0);
        assert_eq!(report.expiring_soon, 0);
        assert_eq!(report.expired, 0);
        assert!(report.items.is_empty());
    }

    #[test]
    fn health_level_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&HealthLevel::ExpiringSoon).expect("序列化失败"),
            "\"expiring_soon\""
        );
        assert_eq!(
            serde_json::to_string(&HealthLevel::Expired).expect("序列化失败"),
            "\"expired\""
        );
    }
}
