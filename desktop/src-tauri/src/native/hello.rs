//! Windows Hello 快速解锁因子（S1）。
//!
//! 仅作在场确认因子，不派生密钥；未启用或取消时一律 fail-closed 返回 `Err(NativeError)`。

use super::NativeError;

const ERR_HELLO_UNAVAILABLE: NativeError =
    NativeError::new(0x8009_0030, "Windows Hello 当前不可用或未完成生物识别确认");
const ERR_EMPTY_REASON: NativeError =
    NativeError::new(0x8007_0057, "Windows Hello 验证原因不能为空");

#[derive(Debug, Clone, Copy, Default)]
pub struct WindowsHelloUnlock {
    simulated_verified: bool,
}

impl WindowsHelloUnlock {
    pub const fn new() -> Self {
        Self {
            simulated_verified: false,
        }
    }

    pub const fn with_verified_state(verified: bool) -> Self {
        Self {
            simulated_verified: verified,
        }
    }

    pub fn available(&self) -> Result<bool, NativeError> {
        Ok(self.simulated_verified)
    }

    pub fn verify(&self, reason: &str) -> Result<(), NativeError> {
        if reason.trim().is_empty() {
            return Err(ERR_EMPTY_REASON);
        }
        if self.simulated_verified {
            Ok(())
        } else {
            Err(ERR_HELLO_UNAVAILABLE)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_unlock_is_fail_closed_by_default() {
        let hello = WindowsHelloUnlock::new();
        assert_eq!(hello.available(), Ok(false));
        assert!(hello.verify("解锁 AI-Bridge").is_err());

        let verified = WindowsHelloUnlock::with_verified_state(true);
        assert_eq!(verified.available(), Ok(true));
        assert!(verified.verify("").is_err());
        assert_eq!(verified.verify("解锁 AI-Bridge"), Ok(()));
    }
}
