//! DPAPI 保护的本地存储 + CNG 随机源 —— core `VaultStore` / `Entropy` 的 Windows 实现。
//!
//! 对应功能点：A1（加密库）、S1（解锁）、S6（明文纪律）。
//! 安全纪律（本层必须自己守住的部分）：
//! - DPAPI 一律按「当前用户」作用域 + `CRYPTPROTECT_UI_FORBIDDEN`，不用机器作用域；
//! - 明文只在内存中停留最短时间，写盘前必为密文（库内容加密本身由 core 负责，
//!   本层再包一层 DPAPI：对应安全模型 §1.2 T2「DPAPI 绑定本机」）；
//! - 日志 / 错误信息绝不输出密钥、明文与完整路径（`NativeError.message` 是静态串）。
//!
//! 接入方式（core 的注入 trait，本层不重复声明 trait）：
//! - `impl ab_vault::VaultStore for DpapiVaultStore`（读写库文件）；
//! - `impl ab_crypto::Entropy for CngEntropy`（系统熵源，禁止用户态算法替代）。
//!
//! 另提供 [`protect`] / [`unprotect`] 原语：恢复密钥（S3）与配对身份秘密的静态封装
//! 也走同一 DPAPI 口径（同一规则只写一份）。
//!
//! 验证状态：DPAPI / BCryptGenRandom 已实跑单测（`dpapi_protect_unprotect_roundtrip` 等）；
//! 断电写坏库的原子替换语义（临时文件 + `std::fs::rename` 覆盖）未做故障注入验证。

use std::path::PathBuf;

use ab_crypto::{CryptoError, Entropy};
use ab_vault::{VaultError, VaultStore};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HLOCAL, LocalFree};
use windows::Win32::Security::Cryptography::{
    BCryptGenRandom, CryptProtectData, CryptUnprotectData, BCRYPT_USE_SYSTEM_PREFERRED_RNG,
    CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};

use super::NativeError;

/// Windows 错误码 `ERROR_FILE_NOT_FOUND`（load 对「库还不存在」返回 None）。
const ERROR_FILE_NOT_FOUND: u32 = 2;

/// DPAPI 包封（当前用户作用域；`CRYPTPROTECT_UI_FORBIDDEN` 保证绝不出系统弹窗）。
///
/// 输出由 DPAPI 分配，拷贝进 `Vec` 后立即 `LocalFree`；明文不复制第二份。
pub fn protect(plaintext: &[u8]) -> Result<Vec<u8>, NativeError> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(plaintext.len()).map_err(|_| NativeError::new(87, "数据长度超出 DPAPI 上限"))?,
        pbData: plaintext.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };

    // SAFETY：input 指向调用方切片（生命周期覆盖本调用）；output 由 DPAPI 分配，
    // 随后按 cbData 拷贝并 LocalFree，不逃逸。
    unsafe {
        CryptProtectData(
            &input,
            PCWSTR::null(),
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
        .map_err(|error| NativeError::new(error.code().0 as u32, "DPAPI 加密失败"))?;
        let sealed = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(Some(HLOCAL(output.pbData as *mut _)));
        Ok(sealed)
    }
}

/// DPAPI 拆封（同作用域）；密文被篡改 / 换用户一律报错，不输出半解密内容。
pub fn unprotect(sealed: &[u8]) -> Result<Vec<u8>, NativeError> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(sealed.len()).map_err(|_| NativeError::new(87, "数据长度超出 DPAPI 上限"))?,
        pbData: sealed.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };

    // SAFETY：同 protect；CryptUnprotectData 失败时无输出可释放。
    unsafe {
        CryptUnprotectData(
            &input,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
        .map_err(|error| NativeError::new(error.code().0 as u32, "DPAPI 解密失败"))?;
        let plain = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(Some(HLOCAL(output.pbData as *mut _)));
        Ok(plain)
    }
}

/// 库存储（core `ab_vault::VaultStore` 的实现）。
///
/// 文件内容 = DPAPI(KDBX4 库字节)：core 负责库内加密，本层负责「本机用户绑定」的外壳。
pub struct DpapiVaultStore {
    path: PathBuf,
}

impl DpapiVaultStore {
    /// 指定库文件路径（调用方决定目录，典型 `%APPDATA%\AI-Bridge\vault.aibvault`）。
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// 读取并拆封库文件；文件不存在返回 `None`（首次建库前的正常状态）。
    ///
    /// 对应 `VaultStore::read`：只返回字节，库内容由 core 解读（本层不解析库内容）。
    pub fn load(&self) -> Result<Option<Vec<u8>>, NativeError> {
        let sealed = match std::fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(NativeError::new(ERROR_FILE_NOT_FOUND, "库文件读取失败")),
        };
        unprotect(&sealed).map(Some)
    }

    /// 写入库文件（DPAPI 包封 + 临时文件原子替换）：先写同目录临时文件再覆盖，
    /// 避免断电写坏库。不写日志明细路径。
    pub fn store(&self, data: &[u8]) -> Result<(), NativeError> {
        let sealed = protect(data)?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|_| NativeError::new(ERROR_FILE_NOT_FOUND, "库目录创建失败"))?;
        }
        let tmp = self.path.with_extension("tmp");
        std::fs::write(&tmp, &sealed).map_err(|_| NativeError::new(ERROR_FILE_NOT_FOUND, "库文件写入失败"))?;
        std::fs::rename(&tmp, &self.path)
            .map_err(|_| NativeError::new(ERROR_FILE_NOT_FOUND, "库文件替换失败"))
    }

    /// 删除库文件。是否软删 / 进回收站由 core 决定，本层只做文件系统删除。
    pub fn remove(&self) -> Result<(), NativeError> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(NativeError::new(ERROR_FILE_NOT_FOUND, "库文件删除失败")),
        }
    }
}

impl VaultStore for DpapiVaultStore {
    fn read(&mut self) -> Result<Option<Vec<u8>>, VaultError> {
        self.load().map_err(|_| VaultError::Store)
    }

    fn write(&mut self, data: &[u8]) -> Result<(), VaultError> {
        self.store(data).map_err(|_| VaultError::Store)
    }
}

/// CNG 随机源（core `ab_crypto::Entropy` 的实现）。与 DPAPI 同属 Windows 密钥区能力，故同文件。
/// 句柄语义（系统 API 的零大小入口），可自由复制注入多处。
#[derive(Clone, Copy)]
pub struct CngEntropy;

impl CngEntropy {
    /// 填充随机字节；对应 `Entropy::fill`。禁止用 `rand` 类的用户态算法替代系统熵源。
    pub fn fill(&self, out: &mut [u8]) -> Result<(), NativeError> {
        // SAFETY：BCryptGenRandom 向调用方切片写入随机字节，无别名逃逸。
        let status = unsafe { BCryptGenRandom(None, out, BCRYPT_USE_SYSTEM_PREFERRED_RNG) };
        if status.0 != 0 {
            return Err(NativeError::new(status.0 as u32, "系统随机源不可用"));
        }
        Ok(())
    }
}

impl Entropy for CngEntropy {
    fn fill(&self, out: &mut [u8]) -> Result<(), CryptoError> {
        CngEntropy::fill(self, out).map_err(|_| CryptoError::EntropyFailed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ab_crypto::random_bytes;

    fn temp_store(name: &str) -> DpapiVaultStore {
        let dir = std::env::var_os("AIBRIDGE_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("test-tmp"))
            .join(format!("aibridge-dpapi-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        DpapiVaultStore::new(dir.join(name))
    }

    #[test]
    fn dpapi_protect_unprotect_roundtrip() {
        let plain = "库明文 · secret-123".as_bytes();
        let sealed = protect(plain).expect("DPAPI 加密失败");
        assert_ne!(sealed, plain, "密文不得等于明文");
        assert_eq!(unprotect(&sealed).expect("DPAPI 解密失败"), plain);
    }

    #[test]
    fn dpapi_tampered_ciphertext_rejected() {
        let mut sealed = protect(b"data").expect("DPAPI 加密失败");
        let mid = sealed.len() / 2;
        sealed[mid] ^= 0x01;
        assert!(unprotect(&sealed).is_err(), "篡改密文必须整体拒绝");
    }

    #[test]
    fn vault_store_roundtrip_and_missing_is_none() {
        let store = temp_store("vault.aibvault");
        let _ = store.remove();
        assert_eq!(store.load().expect("读取失败"), None, "首次读取应为不存在");

        store.store(b"kdbx-bytes").expect("写入失败");
        assert_eq!(store.load().expect("读取失败"), Some(b"kdbx-bytes".to_vec()));

        store.remove().expect("删除失败");
        assert_eq!(store.load().expect("读取失败"), None, "删除后应为不存在");
    }

    #[test]
    fn cng_entropy_fills_through_core_trait() {
        let entropy = CngEntropy;
        let mut out = [0u8; 32];
        random_bytes(&entropy, &mut out).expect("熵源填充失败");
        assert!(out.iter().any(|byte| *byte != 0), "随机输出不应当全零");
    }
}
