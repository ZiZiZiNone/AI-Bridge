//! 托盘图标 + 菜单（W1）：常驻托盘，图标三态（已锁 / 就绪 / 告警）。
//!
//! 规则：菜单清单的唯一数据在 `menu.rs`；点击行为只转交窗口管理或 core，
//! 本层不判断业务（是否已解锁、是否有告警由 core 的会话态决定）。
//!
//! 未验证：`install` / `set_state` 为骨架（TrayIconBuilder 与菜单 API 未编译验证）。

pub mod menu;

use tauri::AppHandle;

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
    /// 图标资源口径（实现期放 `icons/` 下；骨架期只固定命名规则）。
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
/// 骨架说明：菜单点击后的动作只有三类 ——
/// 打开窗口（`menu::MenuItem::window_label`）、切会话态（core）、退出应用；
/// 这三类都不含业务判断，故本函数只做装配。
pub fn install(_app: &AppHandle) -> tauri::Result<()> {
    todo!("按 menu::MENU_ITEMS 构建菜单；on_menu_event 依 id 分派到 ui::window / core / 退出")
}

/// 更新托盘图标与 tooltip（三态切换），由 core 的会话态事件驱动。
pub fn set_state(_app: &AppHandle, _state: TrayState) -> tauri::Result<()> {
    todo!("按 TrayState::icon_name / tooltip 刷新托盘图标")
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
