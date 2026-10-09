//! 面板组（W3–W10）宿主骨架：弹出面板，用完即关。
//!
//! 分工：面板数据一律经核心层取得，本层只声明「哪些是面板」与「怎么开 / 怎么关」，
//! 不含任何业务判断（阈值、到期、健康、验签结果全部来自 core）。
//!
//! 未验证：骨架未经编译验证。

use super::window;
use tauri::AppHandle;

/// 面板窗口 label（顺序同 W3–W10）。几何与形态的权威声明在 `window::WINDOW_SPECS`。
pub const PANEL_LABELS: &[&str] = &[
    "panel-library",    // W3 库管理
    "panel-status",     // W4 状态清单
    "panel-sessions",   // W5 临时会话
    "panel-audit",      // W6 审计
    "panel-backup",     // W7 备份
    "panel-converters", // W8 转换器
    "panel-pairing",    // W9 配对
    "panel-settings",   // W10 设置
];

/// 面板短名 → 窗口 label（`aibridge://panel/<短名>` 用，见 native::protocol）。
pub fn label_of(short_name: &str) -> Option<&'static str> {
    PANEL_LABELS
        .iter()
        .copied()
        .find(|label| label.strip_prefix(window::PANEL_PREFIX) == Some(short_name))
}

/// 打开面板：弹出面板一律「用完即关」，不做多开（已存在则前置）。
pub fn open(app: &AppHandle, label: &str) -> Result<(), String> {
    if !PANEL_LABELS.contains(&label) {
        return Err(format!("不是面板窗口：{label}"));
    }
    window::ensure_open(app, label)
}

/// 关闭面板：隐藏而非销毁（保留 WebView，下次打开更快；实例随应用退出回收）。
pub fn close(app: &AppHandle, label: &str) -> Result<(), String> {
    if !PANEL_LABELS.contains(&label) {
        return Err(format!("不是面板窗口：{label}"));
    }
    window::hide(app, label)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::window::WINDOW_SPECS;

    #[test]
    fn panel_labels_exist_in_window_specs() {
        assert_eq!(PANEL_LABELS.len(), 8);
        for label in PANEL_LABELS {
            assert!(
                WINDOW_SPECS.iter().any(|spec| spec.label == *label),
                "面板 {label} 未在窗口清单中声明"
            );
        }
    }

    #[test]
    fn short_name_maps_back_to_label() {
        assert_eq!(label_of("library"), Some("panel-library"));
        assert_eq!(label_of("does-not-exist"), None);
    }
}
