//! UI Automation 填表与前台焦点读取（B3.1 b 级退路 / B6.2 窗口匹配 / B7.2 一键填充 / D2 带态启动）。
//!
//! 边界（docs/功能点全景.md §5）：站点专属逻辑一律属转换器（A3.3「自动刷新仅转换器可选能力」），
//! 本层只做通用注入原语；是否需要「每次填充前确认」（S4）由 core 决定，本层不判断。
//!
//! 接入方式（core 落地后）：核心的填表输出通道 trait 实现于本类型。
//!
//! 未验证：todo!() 级骨架，未编译、未调用 UIA / SendInput。

use super::NativeError;

/// 前台焦点信息（宿主侧输入，供 core 做窗口 / 域名匹配）。
///
/// 字段只放「原始事实」，不放判断结论（匹配规则、白名单比对在 core）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusInfo {
    /// 前台窗口标题
    pub window_title: String,
    /// 前台进程可执行文件名（如 chrome.exe）
    pub process_name: String,
    /// 可解析出的 URL / 域名线索（无则空串）；解析与匹配规则在 core
    pub url_hint: String,
}

/// UIA 填表原语。
pub struct UiaTyping;

impl UiaTyping {
    /// 读取前台焦点信息。
    pub fn focus_info(&self) -> Result<FocusInfo, NativeError> {
        todo!("GetForegroundWindow + GetWindowThreadProcessId + UIA ElementFromHandle")
    }

    /// 向当前焦点控件填入文本：优先 UIA `ValuePattern`，不可用时退路 `SendInput`。
    pub fn fill_focused(&self, _value: &str) -> Result<(), NativeError> {
        todo!("UIA ValuePattern::SetValue；不可用时 SendInput 逐字符输入")
    }

    /// 提交（模拟回车）。仅用于「一键填充并提交」类动作，是否提交由 core 决定。
    pub fn submit(&self) -> Result<(), NativeError> {
        todo!("SendInput VK_RETURN")
    }
}
