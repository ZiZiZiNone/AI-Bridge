//! 剪贴板原语（B5.1 一次性复制 / B5.2 剪贴板清空 / S6 明文纪律）。
//!
//! 分工（重要）：「30 秒自动清空」「时长可调」「只清空未被用户改动过的剪贴板」都是**业务规则**，
//! 一律写在 core；本层只提供 set / read / clear / sequence 四个系统原语。
//! `sequence_number` 就是给 core 判断「内容是否已被改过」用的输入，判定逻辑不在这里。
//!
//! 接入方式（core 落地后）：核心的填充输出通道 trait 实现于本类型。
//!
//! 未验证：todo!() 级骨架，未编译、未调用 Windows API。

use super::NativeError;

/// 剪贴板原语。
pub struct Clipboard;

impl Clipboard {
    /// 写入文本（一次性复制用户名 / 密码 / 验证码）。
    pub fn set_text(&self, _text: &str) -> Result<(), NativeError> {
        todo!("OpenClipboard + EmptyClipboard + SetClipboardData(CF_UNICODETEXT)")
    }

    /// 读取当前文本。
    ///
    /// 只用于核对「是否仍是我方写入的内容」（判定在 core）；读取结果不得写日志。
    pub fn read_text(&self) -> Result<Option<String>, NativeError> {
        todo!("GetClipboardData(CF_UNICODETEXT)")
    }

    /// 清空剪贴板。
    pub fn clear(&self) -> Result<(), NativeError> {
        todo!("EmptyClipboard")
    }

    /// 剪贴板序列号：内容变化即递增，core 用它与写入时的值比对。
    pub fn sequence_number(&self) -> Result<u32, NativeError> {
        todo!("GetClipboardSequenceNumber")
    }
}
