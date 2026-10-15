//! 托盘菜单清单（W1 的唯一事实源）。
//!
//! 对应关系：本清单与 `desktop/src/ui/tray/TrayMenu.vue` 的清单一一对应，
//! 由 `npm run check:structure` 校验（改这里就要同步改视图，或反之）。
//! 顺序即展示顺序；`id` 是对外稳定标识（前端与 core 只认 id，不认中文标签）。

/// 一个菜单项：id 为稳定标识，label 为展示文案。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuItem {
    pub id: &'static str,
    pub label: &'static str,
}

/// 托盘菜单（docs/功能点全景.md §3.1 W1 的菜单项）。
pub const MENU_ITEMS: &[MenuItem] = &[
    MenuItem {
        id: "unlock-lock",
        label: "解锁 / 锁定",
    },
    MenuItem {
        id: "quick-fill",
        label: "快捷填充",
    },
    MenuItem {
        id: "library",
        label: "库",
    },
    MenuItem {
        id: "status",
        label: "状态清单",
    },
    MenuItem {
        id: "sessions",
        label: "临时会话",
    },
    MenuItem {
        id: "audit",
        label: "审计",
    },
    MenuItem {
        id: "backup",
        label: "备份",
    },
    MenuItem {
        id: "converters",
        label: "转换器",
    },
    MenuItem {
        id: "pairing",
        label: "配对",
    },
    MenuItem {
        id: "settings",
        label: "设置",
    },
    MenuItem {
        id: "quit",
        label: "退出",
    },
];

impl MenuItem {
    /// 菜单项对应的窗口 label；`None` 表示不是「打开窗口」类动作
    /// （如 unlock-lock 切会话态、quick-fill 唤起候选条、quit 退出）。
    pub fn window_label(self) -> Option<&'static str> {
        match self.id {
            "library" => Some("panel-library"),
            "status" => Some("panel-status"),
            "sessions" => Some("panel-sessions"),
            "audit" => Some("panel-audit"),
            "backup" => Some("panel-backup"),
            "converters" => Some("panel-converters"),
            "pairing" => Some("panel-pairing"),
            "settings" => Some("panel-settings"),
            _ => None,
        }
    }

    /// 菜单点击动作（机械映射，不含业务判断）。
    pub fn action(self) -> MenuAction {
        if let Some(label) = self.window_label() {
            return MenuAction::OpenWindow(label);
        }
        match self.id {
            "quick-fill" => MenuAction::SummonCandidate,
            "unlock-lock" => MenuAction::ToggleLock,
            "quit" => MenuAction::Quit,
            _ => MenuAction::None,
        }
    }
}

/// 托盘菜单动作（三类 + 无动作）：打开窗口 / 唤起候选条 / 切会话态 / 退出。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    OpenWindow(&'static str),
    SummonCandidate,
    ToggleLock,
    Quit,
    None,
}

/// 按菜单 id 查动作（未知 id = 无动作，不 panic）。
pub fn action_of(id: &str) -> MenuAction {
    MENU_ITEMS
        .iter()
        .find(|item| item.id == id)
        .map(|item| item.action())
        .unwrap_or(MenuAction::None)
}

#[cfg(test)]
mod tests {
    // 纯数据校验：不启宿主。未验证：本机无 cargo，未运行。
    use super::*;
    use crate::ui::window::WINDOW_SPECS;

    #[test]
    fn menu_items_are_unique_and_match_interface_list() {
        assert_eq!(MENU_ITEMS.len(), 11, "W1 菜单项共 11 项");
        for (index, item) in MENU_ITEMS.iter().enumerate() {
            for other in &MENU_ITEMS[index + 1..] {
                assert_ne!(item.id, other.id, "菜单 id 重复：{}", item.id);
            }
            assert!(!item.label.is_empty());
        }
    }

    #[test]
    fn window_menu_items_point_at_declared_windows() {
        for item in MENU_ITEMS {
            if let Some(label) = item.window_label() {
                assert!(
                    WINDOW_SPECS.iter().any(|spec| spec.label == label),
                    "菜单项 {} 指向未声明的窗口 {}",
                    item.id,
                    label
                );
            }
        }
    }

    #[test]
    fn every_menu_item_maps_to_an_action() {
        assert_eq!(action_of("library"), MenuAction::OpenWindow("panel-library"));
        assert_eq!(action_of("quick-fill"), MenuAction::SummonCandidate);
        assert_eq!(action_of("unlock-lock"), MenuAction::ToggleLock);
        assert_eq!(action_of("quit"), MenuAction::Quit);
        assert_eq!(action_of("does-not-exist"), MenuAction::None);
    }
}
