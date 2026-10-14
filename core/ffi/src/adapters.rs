//! 注入适配：把宿主实现的 UDL 回调 trait 适配成 core 各注入 trait。
//!
//! 全部为签名级搬运；两处形状差异按固定约定桥接（见 README「桥接约定」）：
//! - core `ab_vault::VaultStore` 是「单库字节块」语义，UDL `VaultStore` 是键值面 →
//!   固定键 [`VAULT_BLOB_KEY`] 存取；
//! - core `ab_state::SessionSource::read` 需要 `SessionRecord`（含 account_ref），
//!   UDL `SessionSource::capture` 只给 `Cookie[]` —— 缺 account_ref / source_type 语义，
//!   无法机械构造（UDL↔core 漂移），读取返回明确失败，绝不编造字段。

use std::sync::Arc;

use ab_contract as contract;
use ab_crypto as crypto;
use ab_state as state;
use ab_sync as sync;
use ab_vault as vault;

use crate::traits::{Clock, Entropy, SessionSource, Transport, VaultStore};
use crate::types::CoreError;

/// UDL `VaultStore` 键值面承载 KDBX 库字节块的固定键（core `ab_vault::VaultStore` 无键）。
pub const VAULT_BLOB_KEY: &str = "vault.kdbx";

/// `SessionSource::read` 的漂移失败文案（无秘密）。
pub(crate) const SOURCE_READ_DRIFT: &str =
    "UDL SessionSource.capture 仅返回 Cookie[]，core SessionSource::read 需要 SessionRecord \
     （含 account_ref / source_type），无法机械构造（见 core/ffi/README.md 漂移表）";

/// UDL `Entropy` → core `ab_crypto::Entropy`。
pub(crate) struct EntropyAdapter(pub Arc<dyn Entropy>);

impl crypto::Entropy for EntropyAdapter {
    fn fill(&self, out: &mut [u8]) -> Result<(), crypto::CryptoError> {
        let got = self.0.fill(out.len() as u32);
        if got.len() != out.len() {
            return Err(crypto::CryptoError::EntropyFailed);
        }
        out.copy_from_slice(&got);
        Ok(())
    }
}

/// UDL `Clock` → core `ab_state::Clock`。
pub(crate) struct ClockAdapter(pub Arc<dyn Clock>);

impl state::Clock for ClockAdapter {
    fn now_unix_secs(&self) -> i64 {
        self.0.now_unix_secs()
    }
}

/// UDL `VaultStore`（键值）→ core `ab_vault::VaultStore`（单字节块，固定键）。
pub(crate) struct BlobStoreAdapter(pub Arc<dyn VaultStore>);

impl vault::VaultStore for BlobStoreAdapter {
    fn read(&mut self) -> Result<Option<Vec<u8>>, vault::VaultError> {
        match self.0.read(VAULT_BLOB_KEY.to_string()) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(CoreError::NOT_FOUND) => Ok(None),
            Err(_) => Err(vault::VaultError::Store),
        }
    }

    fn write(&mut self, data: &[u8]) -> Result<(), vault::VaultError> {
        self.0
            .write(VAULT_BLOB_KEY.to_string(), data.to_vec())
            .map_err(|_| vault::VaultError::Store)
    }
}

/// UDL `SessionSource` → core `ab_state::SessionSource`（读取语义缺失，明确失败）。
/// 字段保留宿主源句柄（注入面消费点）；`read` 受漂移阻断，暂无读取路径消费它。
#[allow(dead_code)]
pub(crate) struct SourceAdapter(pub Arc<dyn SessionSource>);

impl state::SessionSource for SourceAdapter {
    fn read(&self, _endpoint: &state::Endpoint) -> Result<Vec<contract::SessionRecord>, state::StateError> {
        Err(state::StateError::Source(SOURCE_READ_DRIFT.to_string()))
    }
}

/// UDL `Transport` → core `ab_sync::Transport`（core 只有 send；peers/receive 无 core 消费点）。
pub(crate) struct TransportAdapter(pub Arc<dyn Transport>);

impl sync::Transport for TransportAdapter {
    fn send(&self, device_id: &str, bytes: &[u8]) -> Result<(), sync::SyncError> {
        self.0
            .send(device_id.to_string(), bytes.to_vec())
            .map_err(|_| sync::SyncError::Transport)
    }
}
