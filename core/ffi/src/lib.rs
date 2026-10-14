//! `ab-ffi` —— AI-Bridge 的 UniFFI 绑定层（`mobile/ffi/aibridge.udl` 的机械镜像）。
//!
//! 纪律（见 `core/ffi/README.md`）：
//! - 接口 / 记录 / 错误逐个映射到 core 各 crate 的公开 API，只转发，零业务规则；
//! - 宿主注入面（`VaultStore` / `SessionSource` / `Transport` / `Clock` / `Entropy`）
//!   由 UniFFI callback interface（`[Trait, WithForeign]`）承接 Swift / Kotlin 实现；
//! - 无单例式全局访问器：core 对象由 `new_core` 显式构造并注入；
//! - 秘密不进 `Debug` / 错误文本（脱敏口径与 core 一致）。

mod adapters;
mod convert;
mod services;
mod traits;
mod types;

pub use adapters::VAULT_BLOB_KEY;
pub use services::{
    generator_entropy_bits, new_core, passphrase_entropy_bits, AibridgeCore, AuditLog, Broker,
    ConverterHost, McpServer, RecoveryKey, StateBus, SyncEngine, Vault,
};
pub use traits::{Clock, Entropy, SessionSource, Transport, VaultStore};
pub use types::*;

uniffi::include_scaffolding!("aibridge");
