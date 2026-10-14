//! 剪贴板原语（B5.1 一次性复制 / B5.2 剪贴板清空 / S6 明文纪律）。
//!
//! 分工（重要）：「30 秒自动清空」「时长可调」「只清空未被用户改动过的剪贴板」都是**业务规则**，
//! 设计上写在 core；本层只提供 set / read / clear / sequence 四个系统原语。
//! `sequence_number` 就是给 core 判断「内容是否已被改过」用的输入，判定逻辑不在这里。
//! （core 现状：无剪贴板策略 API，B5.2 清空策略未接线 —— 见 desktop/README.md core 缺口清单。）
//!
//! 待确认（Win32 语义）：剪贴板被其它进程占用时 `OpenClipboard` 会失败；本层按固定次数
//! 短退避重试（系统原语层的可用性处理，不含业务判断），重试次数与间隔未与产品约定。
//!
//! 验证状态：set/read/clear/sequence 实跑单测（真实 Win32 剪贴板）。

use windows::Win32::Foundation::{GlobalFree, HGLOBAL, HANDLE};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber, OpenClipboard,
    SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

use super::NativeError;

/// `CF_UNICODETEXT`（Win32 常量 13）。
const CF_UNICODETEXT: u32 = 13;
/// 剪贴板被占用时的重试次数（待确认：未与产品约定，取保守 8 次 × 10ms）。
const OPEN_RETRIES: usize = 8;

/// 剪贴板原语。
pub struct Clipboard;

impl Clipboard {
    /// 写入文本（一次性复制用户名 / 密码 / 验证码）。
    pub fn set_text(&self, text: &str) -> Result<(), NativeError> {
        let mut utf16: Vec<u16> = text.encode_utf16().collect();
        utf16.push(0); // 结尾 NUL（CF_UNICODETEXT 约定）

        let bytes = utf16.len() * 2;
        // SAFETY：GlobalAlloc 返回可写内存，写入后交给系统接管（SetClipboardData 成功后
        // 所有权归剪贴板，不再 GlobalFree；失败则本层释放）。
        unsafe {
            let memory = GlobalAlloc(GMEM_MOVEABLE, bytes)
                .map_err(|error| NativeError::new(error.code().0 as u32, "剪贴板内存分配失败"))?;
            let ptr = GlobalLock(memory) as *mut u16;
            if ptr.is_null() {
                let _ = GlobalUnlock(memory);
                let _ = GlobalFree(Some(memory));
                return Err(NativeError::new(8u8 as u32, "剪贴板内存锁定失败"));
            }
            std::ptr::copy_nonoverlapping(utf16.as_ptr(), ptr, utf16.len());
            let _ = GlobalUnlock(memory);

            with_clipboard(|| {
                EmptyClipboard().map_err(|error| NativeError::new(error.code().0 as u32, "剪贴板清空失败"))?;
                SetClipboardData(CF_UNICODETEXT, Some(HANDLE(memory.0)))
                    .map_err(|error| NativeError::new(error.code().0 as u32, "剪贴板写入失败"))?;
                Ok(())
            })
            .map_err(|error| {
                // 持有权未交出：本层释放，不留悬挂内存。
                let _ = GlobalFree(Some(memory));
                error
            })
        }
    }

    /// 读取当前文本。
    ///
    /// 只用于核对「是否仍是我方写入的内容」（判定在 core）；读取结果不得写日志。
    pub fn read_text(&self) -> Result<Option<String>, NativeError> {
        // SAFETY：GetClipboardData 返回的句柄在剪贴板打开期间有效，拷贝成 String 后不保留。
        unsafe {
            with_clipboard(|| {
                let handle = match GetClipboardData(CF_UNICODETEXT) {
                    Ok(handle) => handle,
                    Err(_) => return Ok(None), // 剪贴板当前无文本
                };
                let ptr = GlobalLock(HGLOBAL(handle.0)) as *const u16;
                if ptr.is_null() {
                    let _ = GlobalUnlock(HGLOBAL(handle.0));
                    return Err(NativeError::new(8u8 as u32, "剪贴板内存锁定失败"));
                }
                let mut len = 0usize;
                while *ptr.add(len) != 0 {
                    len += 1;
                }
                let text = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
                let _ = GlobalUnlock(HGLOBAL(handle.0));
                Ok(Some(text))
            })
        }
    }

    /// 清空剪贴板。
    pub fn clear(&self) -> Result<(), NativeError> {
        // SAFETY：仅调用系统清空 API，无内存所有权转移。
        unsafe {
            with_clipboard(|| {
                EmptyClipboard().map_err(|error| NativeError::new(error.code().0 as u32, "剪贴板清空失败"))
            })
        }
    }

    /// 剪贴板序列号：内容变化即递增，core 用它与写入时的值比对。
    pub fn sequence_number(&self) -> Result<u32, NativeError> {
        // SAFETY：纯查询 API，无内存所有权。
        Ok(unsafe { GetClipboardSequenceNumber() })
    }
}

/// 打开剪贴板执行一段操作后关闭；被占用时短退避重试（系统可用性，不含业务判断）。
///
/// # Safety
/// 调用方保证闭包内的 Win32 剪贴板调用只在剪贴板打开期间使用系统句柄。
unsafe fn with_clipboard<T, F>(operation: F) -> Result<T, NativeError>
where
    F: FnOnce() -> Result<T, NativeError>,
{
    let mut opened = false;
    for attempt in 0..OPEN_RETRIES {
        if unsafe { OpenClipboard(None) }.is_ok() {
            opened = true;
            break;
        }
        if attempt + 1 < OPEN_RETRIES {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    if !opened {
        return Err(NativeError::new(5u8 as u32, "剪贴板被其它进程占用"));
    }
    let result = operation();
    let _ = unsafe { CloseClipboard() };
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_set_read_clear_roundtrip() {
        let clipboard = Clipboard;
        let before = clipboard.sequence_number().expect("序列号读取失败");

        clipboard.set_text("一次性复制 · secret").expect("写入失败");
        assert_eq!(
            clipboard.read_text().expect("读取失败").as_deref(),
            Some("一次性复制 · secret")
        );
        assert_ne!(
            clipboard.sequence_number().expect("序列号读取失败"),
            before,
            "内容变化必须递增序列号（core 判定「是否被改过」的输入）"
        );

        clipboard.clear().expect("清空失败");
        assert_eq!(clipboard.read_text().expect("读取失败"), None, "清空后应无文本");
    }
}
