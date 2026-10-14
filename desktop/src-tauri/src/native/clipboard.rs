//! 剪贴板原语（B5.1 一次性复制 / B5.2 剪贴板清空 / S6 明文纪律）。
//!
//! 强制 4 MiB 上界（`MAX_CLIPBOARD_BYTES`），防止恶意外源或未终止缓冲区导致越界读取。

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use super::NativeError;

/// 剪贴板读写最大字节数（4 MiB）。
pub const MAX_CLIPBOARD_BYTES: usize = 4 * 1024 * 1024;
/// UTF-16 码元最大数量（含终止符前正文上界）。
pub const MAX_CLIPBOARD_UTF16_UNITS: usize = MAX_CLIPBOARD_BYTES / 2;

const ERR_TOO_LARGE: NativeError = NativeError::new(0x8007_0057, "剪贴板文本超过 4 MiB 安全上限");
const ERR_UNTERMINATED: NativeError = NativeError::new(0x8007_000D, "剪贴板缓冲区缺少 NUL 终止符");
const ERR_INVALID_UTF16: NativeError = NativeError::new(0x8007_0459, "剪贴板内容不是合法 UTF-16");
const ERR_LOCK_POISONED: NativeError = NativeError::new(0x8000_4005, "剪贴板互斥锁不可用");

/// 校验待写入文本长度，超过 4 MiB 立即拒绝。
pub fn validate_text_len(text: &str) -> Result<(), NativeError> {
    if text.len() > MAX_CLIPBOARD_BYTES {
        return Err(ERR_TOO_LARGE);
    }
    Ok(())
}

/// 从受上界约束的 UTF-16 切片解码文本：要求在 `MAX_CLIPBOARD_UTF16_UNITS` 内命中 NUL 终止符。
pub fn decode_bounded_utf16(units: &[u16]) -> Result<String, NativeError> {
    let scan_limit = units.len().min(MAX_CLIPBOARD_UTF16_UNITS + 1);
    let nul_pos = units[..scan_limit]
        .iter()
        .position(|&u| u == 0)
        .ok_or(if units.len() > MAX_CLIPBOARD_UTF16_UNITS {
            ERR_TOO_LARGE
        } else {
            ERR_UNTERMINATED
        })?;
    if nul_pos * 2 > MAX_CLIPBOARD_BYTES {
        return Err(ERR_TOO_LARGE);
    }
    let decoded = String::from_utf16(&units[..nul_pos]).map_err(|_| ERR_INVALID_UTF16)?;
    validate_text_len(&decoded)?;
    Ok(decoded)
}

#[derive(Debug)]
pub struct Clipboard {
    buffer: Mutex<Option<String>>,
    sequence: AtomicU32,
}

impl Default for Clipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl Clipboard {
    pub fn new() -> Self {
        Self {
            buffer: Mutex::new(None),
            sequence: AtomicU32::new(1),
        }
    }

    pub fn set_text(&self, text: &str) -> Result<(), NativeError> {
        validate_text_len(text)?;
        let mut guard = self.buffer.lock().map_err(|_| ERR_LOCK_POISONED)?;
        *guard = Some(text.to_string());
        self.sequence.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    pub fn read_text(&self) -> Result<Option<String>, NativeError> {
        let guard = self.buffer.lock().map_err(|_| ERR_LOCK_POISONED)?;
        if let Some(ref text) = *guard {
            validate_text_len(text)?;
        }
        Ok(guard.clone())
    }

    pub fn clear(&self) -> Result<(), NativeError> {
        let mut guard = self.buffer.lock().map_err(|_| ERR_LOCK_POISONED)?;
        *guard = None;
        self.sequence.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    pub fn sequence_number(&self) -> Result<u32, NativeError> {
        Ok(self.sequence.load(Ordering::SeqCst))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_read_clear_and_sequence_advance() {
        let cb = Clipboard::new();
        let s0 = cb.sequence_number().unwrap();
        assert_eq!(cb.read_text().unwrap(), None);

        cb.set_text("fixture-secret").unwrap();
        let s1 = cb.sequence_number().unwrap();
        assert!(s1 > s0);
        assert_eq!(cb.read_text().unwrap().as_deref(), Some("fixture-secret"));

        cb.clear().unwrap();
        let s2 = cb.sequence_number().unwrap();
        assert!(s2 > s1);
        assert_eq!(cb.read_text().unwrap(), None);
    }

    #[test]
    fn rejects_payload_exceeding_4mib_limit() {
        let cb = Clipboard::new();
        let oversized = "a".repeat(MAX_CLIPBOARD_BYTES + 1);
        assert_eq!(cb.set_text(&oversized), Err(ERR_TOO_LARGE));
        assert_eq!(cb.read_text().unwrap(), None);

        let units = vec![b'x' as u16; MAX_CLIPBOARD_UTF16_UNITS + 2];
        assert_eq!(decode_bounded_utf16(&units), Err(ERR_TOO_LARGE));
    }

    #[test]
    fn decode_bounded_utf16_requires_nul_terminator() {
        let valid = [b'o' as u16, b'k' as u16, 0, b'z' as u16];
        assert_eq!(decode_bounded_utf16(&valid).unwrap(), "ok");

        let unterminated = [b'n' as u16, b'o' as u16];
        assert_eq!(decode_bounded_utf16(&unterminated), Err(ERR_UNTERMINATED));
    }
}
