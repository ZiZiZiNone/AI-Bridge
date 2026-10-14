//! 面板组（W3–W10）：标签由 `window::WINDOW_SPECS` 唯一派生，不维护第二份硬编码表。

use super::window;

pub fn is_panel(label: &str) -> bool {
    label.starts_with(window::PANEL_PREFIX) && window::find(label).is_some()
}

/// 面板短名 → 窗口 label（供 `aibridge://panel/<短名>` 路由解析）。
pub fn label_of(short_name: &str) -> Option<&'static str> {
    window::WINDOW_SPECS
        .iter()
        .map(|spec| spec.label)
        .find(|label| label.strip_prefix(window::PANEL_PREFIX) == Some(short_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_name_maps_back_to_declared_panel_label() {
        assert_eq!(label_of("library"), Some("panel-library"));
        assert_eq!(label_of("converters"), Some("panel-converters"));
        assert_eq!(label_of("does-not-exist"), None);
        assert!(is_panel("panel-library"));
        assert!(!is_panel("wizard-init"));
    }
}
