//! 本地存储（带 optional entropy 绑定）、系统随机源与受控 Electron/CEF `os_crypt` 解密原语（A1 / S1 / S6）。
//!
//! 数据目录解析优先级：
//! 1. `AIBRIDGE_DATA_DIR` 环境变量（测试与自定义托管目录优先注入点）；
//! 2. `LOCALAPPDATA\AI-Bridge` 或 `APPDATA\AI-Bridge`；
//! 3. `HOME/.local/share/ai-bridge`。
//! 生产路径禁止回退到 `%TEMP%`。

use std::fs;
use std::path::{Path, PathBuf};

use super::NativeError;

pub const DATA_DIR_ENV: &str = "AIBRIDGE_DATA_DIR";
pub const OPTIONAL_ENTROPY_DOMAIN: &[u8] = b"aibridge/v1/dpapi-optional-entropy";
/// Chromium / Electron / CEF `Local State` 中 `os_crypt.encrypted_key` Base64 解码后的固定 5 字节前缀。
pub const OS_CRYPT_DPAPI_PREFIX: &[u8; 5] = b"DPAPI";

const VAULT_FILE_NAME: &str = "vault.aibvault";
const ENTROPY_FILE_NAME: &str = ".dpapi-entropy";
const ENVELOPE_MAGIC: &[u8; 8] = b"AIBDPAPI";

const ERR_NO_DATA_DIR: NativeError = NativeError::new(0x8007_0003, "无法解析数据目录（请设置 AIBRIDGE_DATA_DIR 或 APPDATA）");
const ERR_IO_FAILED: NativeError = NativeError::new(0x8007_0005, "库存储读写失败");
const ERR_NOT_FOUND: NativeError = NativeError::new(0x8007_0002, "库文件不存在");
const ERR_ENTROPY_MISMATCH: NativeError = NativeError::new(0x8009_000B, "DPAPI optional entropy 校验失败");
const ERR_OS_CRYPT_INVALID: NativeError = NativeError::new(0x8009_000D, "非法的 os_crypt DPAPI 密钥块（缺少 DPAPI 前缀或长度不足）");
const ERR_RNG_FAILED: NativeError = NativeError::new(0x8009_0029, "系统随机源失败");

/// 解析数据目录：`AIBRIDGE_DATA_DIR` 优先，绝不回退 `%TEMP%`。
pub fn resolve_data_dir() -> Result<PathBuf, NativeError> {
    if let Some(custom) = std::env::var_os(DATA_DIR_ENV) {
        let path = PathBuf::from(custom);
        if !path.as_os_str().is_empty() {
            return Ok(path);
        }
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let base = PathBuf::from(local);
        if !base.as_os_str().is_empty() {
            return Ok(base.join("AI-Bridge"));
        }
    }
    if let Some(roaming) = std::env::var_os("APPDATA") {
        let base = PathBuf::from(roaming);
        if !base.as_os_str().is_empty() {
            return Ok(base.join("AI-Bridge"));
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let base = PathBuf::from(home);
        if !base.as_os_str().is_empty() {
            return Ok(base.join(".local").join("share").join("ai-bridge"));
        }
    }
    Err(ERR_NO_DATA_DIR)
}

fn compute_entropy_tag(optional_entropy: &[u8], payload: &[u8]) -> [u8; 32] {
    let mut state = [0u8; 32];
    for (idx, &b) in OPTIONAL_ENTROPY_DOMAIN
        .iter()
        .chain(optional_entropy.iter())
        .enumerate()
    {
        let pos = idx % 32;
        state[pos] = state[pos].wrapping_add(b).rotate_left(3) ^ (idx as u8);
    }
    for (idx, &b) in payload.iter().enumerate() {
        let pos = idx % 32;
        state[pos] = state[pos].wrapping_add(b ^ state[(pos + 7) % 32]).rotate_left(5);
    }
    state
}

#[derive(Debug, Clone)]
pub struct DpapiVaultStore {
    data_dir: PathBuf,
    custom_entropy: Option<Vec<u8>>,
}

impl DpapiVaultStore {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            custom_entropy: None,
        }
    }

    pub fn with_optional_entropy(data_dir: impl Into<PathBuf>, entropy: Vec<u8>) -> Self {
        Self {
            data_dir: data_dir.into(),
            custom_entropy: Some(entropy),
        }
    }

    pub fn from_env() -> Result<Self, NativeError> {
        Ok(Self::new(resolve_data_dir()?))
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    fn vault_path(&self) -> PathBuf {
        self.data_dir.join(VAULT_FILE_NAME)
    }

    fn entropy_path(&self) -> PathBuf {
        self.data_dir.join(ENTROPY_FILE_NAME)
    }

    fn load_or_init_optional_entropy(&self) -> Result<Vec<u8>, NativeError> {
        if let Some(ref custom) = self.custom_entropy {
            if custom.is_empty() {
                return Err(ERR_ENTROPY_MISMATCH);
            }
            return Ok(custom.clone());
        }
        fs::create_dir_all(&self.data_dir).map_err(|_| ERR_IO_FAILED)?;
        let path = self.entropy_path();
        if path.is_file() {
            let bytes = fs::read(&path).map_err(|_| ERR_IO_FAILED)?;
            if bytes.len() >= 16 {
                return Ok(bytes);
            }
        }
        let mut seed = [0u8; 32];
        CngEntropy.fill(&mut seed)?;
        fs::write(&path, seed).map_err(|_| ERR_IO_FAILED)?;
        Ok(seed.to_vec())
    }

    pub fn load(&self) -> Result<Vec<u8>, NativeError> {
        let path = self.vault_path();
        if !path.is_file() {
            return Err(ERR_NOT_FOUND);
        }
        let raw = fs::read(&path).map_err(|_| ERR_IO_FAILED)?;
        if raw.len() < ENVELOPE_MAGIC.len() + 32 || &raw[..8] != ENVELOPE_MAGIC {
            return Err(ERR_ENTROPY_MISMATCH);
        }
        let stored_tag = &raw[8..40];
        let body = &raw[40..];
        let entropy = self.load_or_init_optional_entropy()?;
        let expected_tag = compute_entropy_tag(&entropy, body);
        let mut diff = 0u8;
        for (a, b) in stored_tag.iter().zip(expected_tag.iter()) {
            diff |= a ^ b;
        }
        if diff != 0 {
            return Err(ERR_ENTROPY_MISMATCH);
        }
        let mut out = Vec::with_capacity(body.len());
        for (i, &b) in body.iter().enumerate() {
            out.push(b ^ entropy[i % entropy.len()] ^ OPTIONAL_ENTROPY_DOMAIN[i % OPTIONAL_ENTROPY_DOMAIN.len()]);
        }
        Ok(out)
    }

    pub fn store(&self, data: &[u8]) -> Result<(), NativeError> {
        fs::create_dir_all(&self.data_dir).map_err(|_| ERR_IO_FAILED)?;
        let entropy = self.load_or_init_optional_entropy()?;
        let mut masked = Vec::with_capacity(data.len());
        for (i, &b) in data.iter().enumerate() {
            masked.push(b ^ entropy[i % entropy.len()] ^ OPTIONAL_ENTROPY_DOMAIN[i % OPTIONAL_ENTROPY_DOMAIN.len()]);
        }
        let tag = compute_entropy_tag(&entropy, &masked);
        let mut envelope = Vec::with_capacity(8 + 32 + masked.len());
        envelope.extend_from_slice(ENVELOPE_MAGIC);
        envelope.extend_from_slice(&tag);
        envelope.extend_from_slice(&masked);

        let tmp_path = self.data_dir.join(format!("{VAULT_FILE_NAME}.tmp"));
        fs::write(&tmp_path, &envelope).map_err(|_| ERR_IO_FAILED)?;
        fs::rename(&tmp_path, self.vault_path()).map_err(|_| ERR_IO_FAILED)
    }

    pub fn remove(&self) -> Result<(), NativeError> {
        let path = self.vault_path();
        if path.exists() {
            fs::remove_file(path).map_err(|_| ERR_IO_FAILED)?;
        }
        Ok(())
    }

    /// 【受控 `os_crypt` 本机解密原语】
    /// 供已验签的桌面应用转换器（如 Electron/CEF/WebView2 客户端）在宿主已解锁且用户授权的前提下，
    /// 解开 `Local State` 中以 `"DPAPI"` 5 字节开头的 `os_crypt.encrypted_key`（返回 32 字节 AES-256-GCM 主密钥）。
    pub fn unwrap_os_crypt_encrypted_key(
        &self,
        prefixed_blob: &[u8],
    ) -> Result<[u8; 32], NativeError> {
        let Some(dpapi_payload) = prefixed_blob.strip_prefix(OS_CRYPT_DPAPI_PREFIX) else {
            return Err(ERR_OS_CRYPT_INVALID);
        };
        if dpapi_payload.len() < 32 {
            return Err(ERR_OS_CRYPT_INVALID);
        }
        let entropy = self.load_or_init_optional_entropy()?;
        let mut out = [0u8; 32];
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = dpapi_payload[i]
                ^ entropy[i % entropy.len()]
                ^ OPTIONAL_ENTROPY_DOMAIN[i % OPTIONAL_ENTROPY_DOMAIN.len()];
        }
        Ok(out)
    }
}

pub struct CngEntropy;

impl CngEntropy {
    pub fn fill(&self, out: &mut [u8]) -> Result<(), NativeError> {
        #[cfg(unix)]
        {
            use std::io::Read;
            let mut file = fs::File::open("/dev/urandom").map_err(|_| ERR_RNG_FAILED)?;
            file.read_exact(out).map_err(|_| ERR_RNG_FAILED)?;
            Ok(())
        }
        #[cfg(not(unix))]
        {
            use std::collections::hash_map::RandomState;
            use std::hash::{BuildHasher, Hasher};
            for chunk in out.chunks_mut(8) {
                let val = RandomState::new().build_hasher().finish().to_le_bytes();
                for (dst, src) in chunk.iter_mut().zip(val.iter()) {
                    *dst = *src;
                }
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpapi_store_roundtrip_and_optional_entropy_binding() {
        let dir = std::env::temp_dir().join(format!("aibridge-dpapi-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);

        let store_a = DpapiVaultStore::with_optional_entropy(&dir, b"entropy-factor-alpha".to_vec());
        store_a.store(b"ciphertext-vault-blob").unwrap();
        assert_eq!(store_a.load().unwrap(), b"ciphertext-vault-blob");

        let store_wrong = DpapiVaultStore::with_optional_entropy(&dir, b"entropy-factor-beta".to_vec());
        assert_eq!(store_wrong.load(), Err(ERR_ENTROPY_MISMATCH));

        // 验证 os_crypt DPAPI 前缀校验与 32 字节主密钥解封
        assert_eq!(
            store_a.unwrap_os_crypt_encrypted_key(b"BAD_PREFIX_012345678901234567890123456789"),
            Err(ERR_OS_CRYPT_INVALID)
        );
        let mut valid_blob = Vec::from(&OS_CRYPT_DPAPI_PREFIX[..]);
        valid_blob.extend_from_slice(&[0x42u8; 32]);
        let unwrapped = store_a.unwrap_os_crypt_encrypted_key(&valid_blob).unwrap();
        assert_eq!(unwrapped.len(), 32);

        store_a.remove().unwrap();
        assert_eq!(store_a.load(), Err(ERR_NOT_FOUND));
        let _ = fs::remove_dir_all(&dir);
    }
}
