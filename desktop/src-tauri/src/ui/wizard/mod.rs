//! 向导组（W11–W14）宿主骨架：一次性窗口，用完即焚。
//!
//! 语义差别（相对面板）：向导结束后必须**销毁**而不是隐藏 —— 建库口令、恢复密钥、
//! 指纹确认等界面不留在已初始化的 WebView 里复用（S6 明文纪律：内存最小化）。
//!
//! 未验证：骨架未经编译验证。

use super::window;
use tauri::AppHandle;

/// 向导窗口 label（顺序同 W11–W14）。几何与形态的权威声明在 `window::WINDOW_SPECS`。
pub const WIZARD_LABELS: &[&str] = &[
    "wizard-init",    // W11 初始化向导（建库 / S1 / S3）
    "wizard-pairing", // W12 配对向导（E3）
    "wizard-restore", // W13 恢复向导（A6.9 / A8.3）
    "wizard-drill",   // W14 恢复演练向导（A6.8）
];

/// 打开向导（同一向导同时只允许一个实例；已存在则前置）。
pub fn open(app: &AppHandle, label: &str) -> Result<(), String> {
    if !WIZARD_LABELS.contains(&label) {
        return Err(format!("不是向导窗口：{label}"));
    }
    window::ensure_open(app, label)
}

/// 结束向导：一次性窗口用完即焚（销毁而非隐藏）。
pub fn finish(app: &AppHandle, label: &str) -> Result<(), String> {
    if !WIZARD_LABELS.contains(&label) {
        return Err(format!("不是向导窗口：{label}"));
    }
    window::destroy(app, label)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::window::WINDOW_SPECS;

    #[test]
    fn wizard_labels_exist_in_window_specs() {
        assert_eq!(WIZARD_LABELS.len(), 4);
        for label in WIZARD_LABELS {
            assert!(
                WINDOW_SPECS.iter().any(|spec| spec.label == *label),
                "向导 {label} 未在窗口清单中声明"
            );
        }
    }
}
