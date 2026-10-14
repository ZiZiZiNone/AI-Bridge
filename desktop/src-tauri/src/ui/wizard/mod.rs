//! 向导组（W11–W14）：一次性窗口判定由 `window::WINDOW_SPECS` 唯一派生。

use super::window;

pub fn is_wizard(label: &str) -> bool {
    label.starts_with(window::WIZARD_PREFIX) && window::find(label).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wizard_predicate_matches_declared_wizards() {
        assert!(is_wizard("wizard-init"));
        assert!(is_wizard("wizard-pairing"));
        assert!(is_wizard("wizard-restore"));
        assert!(is_wizard("wizard-drill"));
        assert!(!is_wizard("panel-library"));
        assert!(!is_wizard("wizard-unknown"));
    }
}
