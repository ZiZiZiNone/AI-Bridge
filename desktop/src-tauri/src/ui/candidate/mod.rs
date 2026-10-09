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
/// 骨架：位置计算依赖系统能力（GetCursorPos / UIA caret），实现期接入 `native`；
/// 本层只负责「取到锚点坐标 → 交给窗口 API」。
fn place(_app: &AppHandle, _anchor: ShowAnchor) -> Result<(), String> {
    todo!("取锚点坐标（Pointer=GetCursorPos，ForegroundCaret=UIA caret）后 set_position")
}
