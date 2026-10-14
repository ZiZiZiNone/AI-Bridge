//! `ab-state` —— 会话状态层：清单矩阵 / 覆盖前快照 / 回滚 / 健康分级。
//!
//! 模块职责（`docs/架构与契约.md` §2）：只做会话状态业务规则，依赖只有契约层
//! `ab-contract`。外部能力一律构造注入：存储 [`StateStore`]、时间 [`Clock`]、
//! 端会话读取 [`SessionSource`]。仓库内无全局单例、无静态门面、无服务定位器；
//! 桌面 / 移动 / 单测三个宿主共用同一份实现（搬进单测不改内部代码即可工作）。
//!
//! 与契约的关系：会话本体复用 `ab_contract::SessionRecord`（唯一事实源），结构校验
//! 直接调用其 `validate()`，本 crate 不重复实现契约规则。
//!
//! 明确不做：签发或续期临时会话（`ab-broker`）、加密与传输（`ab-sync`）、文件落盘
//! （由 [`StateStore`] 实现方负责）、审计写入（`ab-audit`）、直读浏览器（只经
//! [`SessionSource`]）。

pub mod clock;
pub mod error;
pub mod health;
pub mod matrix;
pub mod model;
pub mod snapshot;
pub mod source;
pub mod store;

#[cfg(test)]
mod test_support;

pub use clock::{Clock, SystemClock};
pub use error::StateError;
pub use health::{HealthItem, HealthLevel, HealthPolicy, HealthReport, DEFAULT_WARN_WITHIN_SECS};
pub use matrix::{ExpiryFilter, MatrixFilters, MatrixRow, MatrixSort, SortKey};
pub use model::{
    CaptureReport, Endpoint, EndpointKind, RollbackReport, SessionEntry, Snapshot, SnapshotMeta,
};
pub use source::SessionSource;
pub use store::{InMemoryStateStore, StateStore};

/// 状态服务：依赖全部经构造注入，方法取 `&self`（线程同步由实现方自持，无全局状态）。
///
/// - [`StateService::matrix`]：会话矩阵筛选与排序（账号 × 端 × 来源 × 到期）
/// - [`StateService::capture`]：读端现状 + 覆盖前快照 + 写入
/// - [`StateService::snapshot`] / [`StateService::list_snapshots`] / [`StateService::rollback`]
/// - [`StateService::health`]：到期倒计时与告警分级
pub struct StateService<S: StateStore, C: Clock, R: SessionSource> {
    store: S,
    clock: C,
    source: R,
    health_policy: HealthPolicy,
}

impl<S: StateStore, C: Clock, R: SessionSource> StateService<S, C, R> {
    /// 组装：注入存储、时钟、端会话来源与健康策略。
    pub fn new(store: S, clock: C, source: R, health_policy: HealthPolicy) -> Self {
        Self {
            store,
            clock,
            source,
            health_policy,
        }
    }
}
