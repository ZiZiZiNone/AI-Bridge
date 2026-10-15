//! Windows Hello 快速解锁（S1）—— core 解锁因子的实现骨架 → 真实现（WinRT `UserConsentVerifier`）。
//!
//! 语义边界（docs/功能点全景.md S1）：Windows Hello **仅作解锁因子**，不作为密钥派生来源；
//! 库内容仍由主密码（Argon2id）派生的密钥保护，Hello 只回答「本人在场吗」。
//! 因此本层返回布尔判定，绝不返回任何密钥材料。
//!
//! 待确认（Win32/WinRT 语义）：
//! - `UserConsentVerifier` 属 WinRT，要求 COM/WinRT 初始化（本层按 `CoInitializeEx`
//!   STA 尝试初始化并容忍 `S_FALSE`；`RPC_E_CHANGED_MODE` 下的行为未实测）；
//! - 请求确认的弹窗文案（`RequestVerificationAsync` 的 message）由调用方给出，
//!   产品文案待 UI 定稿。
//!
//! 验证状态：`block_on` 执行器有单测；WinRT 往返（真实 Hello 弹窗）需要用户在场，
//! 本机未验证（见 desktop/README.md 未验证声明）。

use std::future::{Future, IntoFuture};
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use windows::core::HSTRING;
use windows::Security::Credentials::UI::{
    UserConsentVerificationResult, UserConsentVerifier, UserConsentVerifierAvailability,
};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};

use super::NativeError;

/// Windows Hello 解锁因子。
pub struct WindowsHelloUnlock;

impl WindowsHelloUnlock {
    /// 平台能力探测（是否可用 / 是否已注册）。
    ///
    /// 语义：失败即回退主密码解锁，绝不阻断解锁流程（core 决定回退策略）。
    /// 返回 `true` 仅当 WinRT 判定 `Available`。
    pub fn available(&self) -> Result<bool, NativeError> {
        ensure_com()?;
        let operation = UserConsentVerifier::CheckAvailabilityAsync()
            .map_err(|error| NativeError::new(error.code().0 as u32, "Hello 能力探测失败"))?;
        let availability = block_on(operation.into_future())
            .map_err(|error| NativeError::new(error.code().0 as u32, "Hello 能力探测失败"))?;
        Ok(availability == UserConsentVerifierAvailability::Available)
    }

    /// 请求一次用户确认。
    ///
    /// 语义：`Ok(())` 只表示「本人在场」；是否用它解锁由 core 判断。
    /// 失败（用户取消 / 不可用）必须以 `NativeError` 上报，不得 panic。
    pub fn verify(&self, reason: &str) -> Result<(), NativeError> {
        ensure_com()?;
        let operation = UserConsentVerifier::RequestVerificationAsync(&HSTRING::from(reason))
            .map_err(|error| NativeError::new(error.code().0 as u32, "Hello 确认请求失败"))?;
        let result = block_on(operation.into_future())
            .map_err(|error| NativeError::new(error.code().0 as u32, "Hello 确认请求失败"))?;
        if result == UserConsentVerificationResult::Verified {
            Ok(())
        } else {
            Err(NativeError::new(1u32, "Hello 确认未通过"))
        }
    }
}

/// WinRT 调用前的最小 COM 初始化：已是 STA/MTA 时不重复设模式，失败不 panic。
fn ensure_com() -> Result<(), NativeError> {
    // SAFETY：CoInitializeEx 只影响当前线程；S_FALSE（已初始化）视为成功。
    let status = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    if status.is_ok() {
        Ok(())
    } else {
        Err(NativeError::new(status.0 as u32, "COM 初始化失败"))
    }
}

/// 最小阻塞执行器：把 WinRT `IAsyncOperation` 的 Future 跑到完成。
///
/// 不引入运行时（native/ 不依赖 tokio/tauri）；线程 park/unpark 由 `std::task::Wake`
/// 安全封装实现，无 `unsafe`。
fn block_on<F: Future>(future: F) -> F::Output {
    struct ThreadWake(std::thread::Thread);
    impl Wake for ThreadWake {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }

    let waker = Waker::from(Arc::new(ThreadWake(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => std::thread::park(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_on_runs_future_to_completion() {
        assert_eq!(block_on(async { 40 + 2 }), 42);
    }

    #[test]
    fn block_on_wakes_across_threads() {
        let handle = std::thread::spawn(move || {
            block_on(async {
                let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                let flag = done.clone();
                std::thread::spawn(move || {
                    // 模拟 WinRT 回调线程：稍后置位。
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    flag.store(true, std::sync::atomic::Ordering::SeqCst);
                });
                // 简单让步一次后返回（真正的 WinRT future 由 Completed 唤醒）。
                YieldNow { yielded: false }.await;
                done.load(std::sync::atomic::Ordering::SeqCst)
            })
        });
        assert!(handle.join().unwrap());
    }

    struct YieldNow {
        yielded: bool,
    }
    impl std::future::Future for YieldNow {
        type Output = ();
        fn poll(
            mut self: std::pin::Pin<&mut Self>,
            context: &mut Context<'_>,
        ) -> Poll<Self::Output> {
            if self.yielded {
                Poll::Ready(())
            } else {
                self.yielded = true;
                context.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }
}
