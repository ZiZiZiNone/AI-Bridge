//! 托盘图标 + 菜单（W1）：常驻托盘，图标三态（已锁 / 就绪 / 告警）。
//!
//! 规则：菜单清单的唯一数据在 `menu.rs`；点击行为只转交窗口管理或 core，
//! 本层不判断业务（是否已解锁、是否有告警由 core 的会话态决定）。
//!
//! 验证状态：TrayIconBuilder / 菜单 / 三态切换的编译形态已验证；真实托盘渲染与
//! 点击行为需图形会话（`cargo tauri dev` / 真实运行），未实测（见 desktop/README.md）。
//!
//! 图标资源：三态专用图标（tray-locked / tray-ready / tray-alert）**暂缺**，
//! 现共用 `icons/32x32.png`（占位图标，见 scripts/make-icons.py）；待设计交付后
//! 按 `TrayState::icon_name()` 的命名规则补入 `src-tauri/icons/`（README 已标注待补）。

pub mod menu;

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};

use crate::host::HostServices;

/// 托盘实例 id（`tray_by_id` 取回用）。
pub const TRAY_ID: &str = "aibridge-tray";

/// 托盘图标资源（占位：三态共用）。
const TRAY_ICON_PNG: &[u8] = include_bytes!("../../../icons/32x32.png");

/// 托盘图标三态。状态由 core 的会话就绪态驱动，本层只承载展示态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayState {
    /// 已锁（未解锁 / 已手动锁定）
    Locked,
    /// 就绪（解锁后进入就绪态）
    Ready,
    /// 告警（会话异常 / 到期提醒）
    Alert,
}

impl TrayState {
    /// 图标资源口径（三态专用图标待补，现共用占位图；见文件头）。
    pub fn icon_name(self) -> &'static str {
        match self {
            TrayState::Locked => "tray-locked.png",
            TrayState::Ready => "tray-ready.png",
            TrayState::Alert => "tray-alert.png",
        }
    }

    /// 托盘 tooltip（显示名一律 AI-Bridge）。
    pub fn tooltip(self) -> &'static str {
        match self {
            TrayState::Locked => "AI-Bridge · 已锁定",
            TrayState::Ready => "AI-Bridge · 就绪",
            TrayState::Alert => "AI-Bridge · 需要处理",
        }
    }
}

/// 安装托盘（图标 + 菜单 + 事件）：菜单项来自 `menu::MENU_ITEMS`。
///
/// 点击行为只有三类（`menu::MenuAction`）：打开窗口、唤起候选条、切会话态 / 退出；
/// 这些都不含业务判断，故本函数只做装配。
pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let mut items = Vec::with_capacity(menu::MENU_ITEMS.len());
    for item in menu::MENU_ITEMS {
        items.push(MenuItem::with_id(app, item.id, item.label, true, None::<&str>)?);
    }
    let item_refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
        items.iter().map(|item| item as _).collect();
    let menu = Menu::with_items(app, &item_refs)?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::from_bytes(TRAY_ICON_PNG)?)
        .tooltip(TrayState::Locked.tooltip()) // 初始 = 已锁（S1 解锁前）
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            match menu::action_of(event.id().as_ref()) {
                menu::MenuAction::OpenWindow(label) => {
                    let _ = crate::ui::window::ensure_open(app, label);
                }
                menu::MenuAction::SummonCandidate => {
                    let _ = crate::ui::candidate::summon(app, crate::ui::candidate::ShowAnchor::Pointer);
                }
                menu::MenuAction::ToggleLock => toggle_lock(app),
                menu::MenuAction::Quit => app.exit(0),
                menu::MenuAction::None => {}
            }
        })
        .build(app)?;
    Ok(())
}

/// 切会话态（W-F1）：就绪态 → 锁定（S7）；已锁 → 打开设置面板（W10 承载 S1 解锁）。
fn toggle_lock(app: &AppHandle) {
    let services = app.state::<HostServices>();
    if crate::commands::library::is_unlocked(&services) {
        let _ = crate::commands::library::lock(&services);
        let _ = set_state(app, TrayState::Locked);
    } else {
        let _ = crate::ui::window::ensure_open(app, "panel-settings");
    }
}

/// 更新托盘图标与 tooltip（三态切换），由 core 的会话态事件驱动。
pub fn set_state(app: &AppHandle, state: TrayState) -> tauri::Result<()> {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return Ok(()); // 托盘未安装（非桌面入口）：无展示态可更新
    };
    tray.set_icon(Some(Image::from_bytes(TRAY_ICON_PNG)?))?;
    tray.set_tooltip(Some(state.tooltip()))?;
    tray.set_title(Some(state.tooltip()))
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

    #[test]
    fn tray_icon_bytes_are_a_png() {
        // 占位图可用性：PNG 魔数 + 非空字节（三态专用图标待补，见文件头）。
        assert!(TRAY_ICON_PNG.len() > 8);
        assert_eq!(&TRAY_ICON_PNG[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    }
}
