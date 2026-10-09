//! 状态层错误。结构一致性错误直接复用契约层 `ContractError`（同一规则只写一份）。

use ab_contract::ContractError;
use std::fmt;

/// ab-state 的错误面：存储 / 来源 / 契约 / 快照缺失 / 条目前置字段非法。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateError {
    /// 存储实现失败（IO、锁中毒等）。
    Store(String),
    /// 端会话读取失败（[`crate::SessionSource`] 实现失败）。
    Source(String),
    /// 契约结构校验失败（[`ab_contract::ContractError`] 唯一实现）。
    Contract(ContractError),
    /// 指定快照不存在。
    SnapshotNotFound(String),
    /// 条目前置字段非法：账号引用 / 端 id 为空，或到期时间不是合法 RFC3339。
    InvalidEntry { field: &'static str, value: String },
}

impl fmt::Display for StateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StateError::Store(msg) => write!(f, "状态存储失败：{msg}"),
            StateError::Source(msg) => write!(f, "端会话读取失败：{msg}"),
            StateError::Contract(err) => write!(f, "契约校验失败：{err}"),
            StateError::SnapshotNotFound(id) => write!(f, "快照不存在：{id}"),
            StateError::InvalidEntry { field, value } => {
                write!(f, "条目字段非法：{field} = {value}")
            }
        }
    }
}

impl std::error::Error for StateError {}

impl From<ContractError> for StateError {
    fn from(err: ContractError) -> Self {
        StateError::Contract(err)
    }
}
