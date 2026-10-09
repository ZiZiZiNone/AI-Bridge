//! DPAPI 保护的本地存储 + CNG 随机源 —— core `VaultStore` / `Entropy` 的 Windows 实现骨架。
//!
//! 对应功能点：A1（加密库）、S1（解锁）、S6（明文纪律）。
//! 安全纪律（本层必须自己守住的部分）：
//! - DPAPI 一律按「当前用户」作用域 + `CRYPTPROTECT_UI_FORBIDDEN`，不用机器作用域；
//! - 明文只在内存中停留最短时间，写盘前必为密文（加密本身由 core 负责，本层只做系统级封装与文件替换）；
//! - 日志 / 错误信息绝不输出密钥、明文与完整路径（`NativeError.message` 是静态串）。
//!
//! 接入方式（core 落地后补 impl 头，方法签名与下方一一对应）：
//! ```ignore
//! impl ab_vault::VaultStore for DpapiVaultStore { /* 转发到同名方法 */ }
//! impl ab_crypto::Entropy for CngEntropy { /* 转发到 fill */ }
//! ```
//!
//! 未验证：todo!() 级骨架，未编译、未调用任何 Windows API。

use super::NativeError;

/// 库存储（core `VaultStore` 的实现骨架）。
pub struct DpapiVaultStore;

impl DpapiVaultStore {
    /// 读取库文件（DPAPI 解包）。
    ///
    /// 对应 `VaultStore::load`：只返回字节，密文结构由 core 解读（本层不解析库内容）。
    pub fn load(&self) -> Result<Vec<u8>, NativeError> {
        todo!("CryptUnprotectData 解包库文件（%APPDATA%\\AI-Bridge\\vault.aibvault）")
    }

    /// 写入库文件（DPAPI 包封 + 原子替换）。
    ///
    /// 对应 `VaultStore::store`：先写同目录临时文件再 `ReplaceFileW`，避免断电写坏库。
    pub fn store(&self, _data: &[u8]) -> Result<(), NativeError> {
        todo!("CryptProtectData 包封后写临时文件，再 ReplaceFileW 原子替换")
    }

    /// 删除库文件。是否软删 / 进回收站由 core 决定，本层只做文件系统删除。
    pub fn remove(&self) -> Result<(), NativeError> {
        todo!("删除库文件（不写日志明细路径）")
    }
}

/// CNG 随机源（core `Entropy` 的实现骨架）。与 DPAPI 同属 Windows 密钥区能力，故同文件。
pub struct CngEntropy;

impl CngEntropy {
    /// 填充随机字节；对应 `Entropy::fill`。禁止用 `rand` 类的用户态算法替代系统熵源。
    pub fn fill(&self, _out: &mut [u8]) -> Result<(), NativeError> {
        todo!("BCryptGenRandom(BCRYPT_USE_SYSTEM_PREFERRED_RNG)")
    }
}
