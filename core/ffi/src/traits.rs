//! 宿主注入面：UDL `[Trait, WithForeign]` 的五个 trait（Swift / Kotlin 侧实现）。
//!
//! 签名与生成脚手架逐字一致；`Send + Sync` 超trait 是 core 各注入 trait 的要求
//! （`ab_crypto::Entropy` / `ab_state::{Clock, SessionSource}` / `ab_sync::Transport`），
//! 使 `Arc<dyn ...>` 可直接被适配进 core。

use crate::types::{Cookie, CoreError, PeerInfo};

/// 加密库存储：只存不透明字节，键值语义由 core 决定。
pub trait VaultStore: Send + Sync {
    fn list_keys(&self) -> Result<Vec<String>, CoreError>;
    fn read(&self, key: String) -> Result<Vec<u8>, CoreError>;
    fn remove(&self, key: String) -> Result<(), CoreError>;
    fn write(&self, key: String, value: Vec<u8>) -> Result<(), CoreError>;
}

/// 会话来源：浏览器扩展 / App 容器（WKWebView / Android WebView）。
/// cookie 属性必须完整往返（会话保真度关键）。
pub trait SessionSource: Send + Sync {
    fn available_sources(&self) -> Vec<String>;
    fn capture(&self, source_id: String, domain: String) -> Vec<Cookie>;
    fn inject(&self, source_id: String, cookies: Vec<Cookie>);
}

/// 传输：局域网直传 / 离线包搬运；core 不持有任何网络句柄。
pub trait Transport: Send + Sync {
    fn peers(&self) -> Vec<PeerInfo>;
    fn receive(&self, timeout_millis: u32) -> Option<Vec<u8>>;
    fn send(&self, peer_id: String, payload: Vec<u8>) -> Result<(), CoreError>;
}

/// 时钟：注入以便单测与 TTL 判定（core 不自取系统时间）。
pub trait Clock: Send + Sync {
    fn now_unix_secs(&self) -> i64;
}

/// 熵源：生成器与密钥派生的随机来源（core 不自取系统随机）。
pub trait Entropy: Send + Sync {
    fn fill(&self, length: u32) -> Vec<u8>;
}
