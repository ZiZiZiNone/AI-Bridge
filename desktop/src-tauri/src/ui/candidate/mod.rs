//! 候选条（W2）：热键唤起与跟随鼠标 / 跟随输入光标（UIA caret）定位（B6.1）。

use super::window;
use crate::native::uia_typing::{ScreenPoint, UiaTyping};
use tauri::{AppHandle, Manager, PhysicalPosition, Position};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShowAnchor {
    /// 跟随鼠标（默认）
    Pointer,
    /// 跟随输入焦点光标（UIA caret）
    ForegroundCaret,
}

pub fn resolve_anchor_point(uia: &UiaTyping, anchor: ShowAnchor) -> Result<ScreenPoint, String> {
    match anchor {
        ShowAnchor::Pointer => uia.pointer_position().map_err(|e| e.to_string()),
        ShowAnchor::ForegroundCaret => uia.caret_position().map_err(|e| e.to_string()),
    }
}

pub fn summon(app: &AppHandle, anchor: ShowAnchor) -> Result<(), String> {
    window::toggle(app, window::CANDIDATE_LABEL)?;
    place(app, anchor)
}

pub fn place(app: &AppHandle, anchor: ShowAnchor) -> Result<(), String> {
    let point = resolve_anchor_point(&UiaTyping::new(), anchor)?;
    if let Some(win) = app.get_webview_window(window::CANDIDATE_LABEL) {
        let visible = win.is_visible().map_err(|e| e.to_string())?;
        if visible {
            win.set_position(Position::Physical(PhysicalPosition::new(point.x, point.y)))
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::uia_typing::FocusInfo;

    #[test]
    fn anchor_resolves_pointer_and_caret_without_panic() {
        let uia = UiaTyping::with_context(
            FocusInfo {
                window_title: "Editor".into(),
                process_name: "code.exe".into(),
                url_hint: String::new(),
            },
            ScreenPoint { x: 120, y: 240 },
            Some(ScreenPoint { x: 400, y: 500 }),
        );
        assert_eq!(
            resolve_anchor_point(&uia, ShowAnchor::Pointer).unwrap(),
            ScreenPoint { x: 120, y: 240 }
        );
        assert_eq!(
            resolve_anchor_point(&uia, ShowAnchor::ForegroundCaret).unwrap(),
            ScreenPoint { x: 400, y: 500 }
        );
    }
}
