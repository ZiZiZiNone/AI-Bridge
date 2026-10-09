//! 端会话读取注入点。
//!
//! ab-state 永不直读浏览器或任何宿主对象：谁读会话（扩展桥、转换器宿主、移动通道），
//! 谁实现本 trait，由宿主在组装时注入。

use ab_contract::SessionRecord;

use crate::error::StateError;
use crate::model::Endpoint;

/// 读取某个端当前的会话集合。
pub trait SessionSource: Send + Sync {
    /// 读取指定端的全部会话（cookie 属性必须完整携带，不得丢字段）。
    fn read(&self, endpoint: &Endpoint) -> Result<Vec<SessionRecord>, StateError>;
}
