//! 候选条（W2）宿主骨架：热键唤起 + 显示锚点。
//!
//! 功能点：B6.1 全局热键 `Ctrl+Alt+P` 唤起候选条，默认跟随鼠标，可切跟随光标。
//! 分工：候选内容的查询与匹配在 core（ab-vault + ab-state），本层只做窗口唤起与定位。
//!
//! 未验证：定位需要 Windows API（GetCursorPos / UIA caret），骨架未实现。

use super::window;
use tauri::AppHandle;

/// 候选条显示锚点（B6.1）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShowAnchor {
    /// 跟随鼠标（默认）
    Pointer,
    /// 跟随光标（前台输入焦点位置）
    ForegroundCaret,
}

/// 唤起候选条：已显示则隐藏（热键为切换语义）。
pub fn summon(app: &AppHandle, anchor: ShowAnchor) -> Result<(), String> {
    window::toggle(app, window::CANDIDATE_LABEL)?;
    place(app, anchor)
}

/// 按锚点定位候选条窗口。
///
/// 实现期接入 `native`（Pointer=GetCursorPos，ForegroundCaret=UIA caret）。
/// 骨架期 fail-closed：定位未实现前不 panic（协议入口 `aibridge://candidate` 与
/// 托盘「快速填充」直达本函数，`todo!()` 会让持有已解锁库的宿主进程崩溃——
/// 审查报告 P0-1）。窗口已由 `summon` 切换显示，仅定位退化为默认位置。
fn place(_app: &AppHandle, anchor: ShowAnchor) -> Result<(), String> {
    // 未接 native 前：Pointer 锚点可退化为屏幕中心；ForegroundCaret 无从取坐标，
    // 显式报错而非 panic（调用方可提示用户）。
    match anchor {
        ShowAnchor::Pointer => Ok(()), // 退化为窗口默认位置
        ShowAnchor::ForegroundCaret => {
            Err("光标锚点定位未实现（待接 UIA caret），请用默认位置".to_string())
        }
    }
}
