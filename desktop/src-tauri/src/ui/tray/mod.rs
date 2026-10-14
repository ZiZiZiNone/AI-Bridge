//! 托盘图标 + 菜单（W1）：常驻托盘，图标三态（已锁 / 就绪 / 告警）。

pub mod menu;

use super::candidate::{self, ShowAnchor};
use super::window;
use tauri::AppHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayState {
    Locked,
    Ready,
    Alert,
}

impl TrayState {
    pub fn icon_name(self) -> &'static str {
        match self {
            TrayState::Locked => "tray-locked.png",
            TrayState::Ready => "tray-ready.png",
            TrayState::Alert => "tray-alert.png",
        }
    }

    pub fn tooltip(self) -> &'static str {
        match self {
            TrayState::Locked => "AI-Bridge · 已锁定",
            TrayState::Ready => "AI-Bridge · 就绪",
            TrayState::Alert => "AI-Bridge · 需要处理",
        }
    }
}

pub fn dispatch_menu_item(app: &AppHandle, item_id: &str) -> Result<(), String> {
    if item_id == "quick-fill" {
        return candidate::summon(app, ShowAnchor::Pointer);
    }
    if item_id == "quit" {
        app.exit(0);
        return Ok(());
    }
    if let Some(item) = menu::MENU_ITEMS.iter().find(|it| it.id == item_id) {
        if let Some(label) = item.window_label() {
            return window::ensure_open(app, label);
        }
    }
    Ok(())
}

pub fn install(_app: &AppHandle) -> tauri::Result<()> {
    Ok(())
}

pub fn set_state(_app: &AppHandle, _state: TrayState) -> tauri::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tri_state_has_distinct_icon_and_tooltip() {
        let states = [TrayState::Locked, TrayState::Ready, TrayState::Alert];
        for (index, state) in states.iter().enumerate() {
            assert!(state.tooltip().starts_with("AI-Bridge"));
            for other in &states[index + 1..] {
                assert_ne!(state.icon_name(), other.icon_name());
            }
        }
    }
}
