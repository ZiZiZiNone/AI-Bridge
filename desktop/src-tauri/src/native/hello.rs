//! Windows Hello 快速解锁（S1）—— core 解锁因子的实现骨架。
//!
//! 语义边界（docs/功能点全景.md S1）：Windows Hello **仅作解锁因子**，不作为密钥派生来源；
//! 库内容仍由主密码（Argon2id）派生的密钥保护，Hello 只回答「本人在场吗」。
//! 因此本层返回布尔判定，绝不返回任何密钥材料。
//!
//! 接入方式（core 落地后）：`impl <core 解锁因子 trait> for WindowsHelloUnlock`
//! （trait 名待 core 定义，本层不重复声明）。
//!
//! 未验证：todo!() 级骨架，未编译、未调用 WinRT。

use super::NativeError;

/// Windows Hello 解锁因子。
pub struct WindowsHelloUnlock;

impl WindowsHelloUnlock {
    /// 平台能力探测（是否可用 / 是否已注册）。
    ///
    /// 语义：失败即回退主密码解锁，绝不阻断解锁流程（core 决定回退策略）。
    pub fn available(&self) -> Result<bool, NativeError> {
        todo!("WinRT UserConsentVerifier::CheckAvailabilityAsync")
    }

    /// 请求一次用户确认。
    ///
    /// 语义：`Ok(())` 只表示「本人在场」；是否用它解锁由 core 判断。
    /// 失败（用户取消 / 不可用）必须以 `NativeError` 上报，不得 panic。
    pub fn verify(&self, _reason: &str) -> Result<(), NativeError> {
        todo!("UserConsentVerifier::RequestVerificationAsync（结果仅作布尔判定）")
    }
}
