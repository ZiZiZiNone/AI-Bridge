//! 窗口管理：窗口清单（唯一事实源）+ 打开 / 隐藏 / 销毁 / 切换。

use serde::Serialize;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const CANDIDATE_LABEL: &str = "candidate";
pub const PANEL_PREFIX: &str = "panel-";
pub const WIZARD_PREFIX: &str = "wizard-";

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct WindowSpec {
    pub w: &'static str,
    pub label: &'static str,
    pub title: &'static str,
    pub width: f64,
    pub height: f64,
    pub resizable: bool,
    pub decorations: bool,
    pub always_on_top: bool,
    pub skip_taskbar: bool,
}

/// 窗口清单（唯一事实源）：13 个窗口 = 候选条 1（W2）+ 面板 8（W3–W10）+ 向导 4（W11–W14）。
pub const WINDOW_SPECS: &[WindowSpec] = &[
    WindowSpec {
        w: "W2",
        label: CANDIDATE_LABEL,
        title: "AI-Bridge 候选条",
        width: 420.0,
        height: 320.0,
        resizable: false,
        decorations: false,
        always_on_top: true,
        skip_taskbar: true,
    },
    WindowSpec {
        w: "W3",
        label: "panel-library",
        title: "AI-Bridge 库",
        width: 760.0,
        height: 580.0,
        resizable: true,
        decorations: true,
        always_on_top: false,
        skip_taskbar: true,
    },
    WindowSpec {
        w: "W4",
        label: "panel-status",
        title: "AI-Bridge 状态清单",
        width: 860.0,
        height: 600.0,
        resizable: true,
        decorations: true,
        always_on_top: false,
        skip_taskbar: true,
    },
    WindowSpec {
        w: "W5",
        label: "panel-sessions",
        title: "AI-Bridge 临时会话",
        width: 760.0,
        height: 560.0,
        resizable: true,
        decorations: true,
        always_on_top: false,
        skip_taskbar: true,
    },
    WindowSpec {
        w: "W6",
        label: "panel-audit",
        title: "AI-Bridge 审计",
        width: 860.0,
        height: 600.0,
        resizable: true,
        decorations: true,
        always_on_top: false,
        skip_taskbar: true,
    },
    WindowSpec {
        w: "W7",
        label: "panel-backup",
        title: "AI-Bridge 备份",
        width: 700.0,
        height: 520.0,
        resizable: true,
        decorations: true,
        always_on_top: false,
        skip_taskbar: true,
    },
    WindowSpec {
        w: "W8",
        label: "panel-converters",
        title: "AI-Bridge 转换器",
        width: 860.0,
        height: 560.0,
        resizable: true,
        decorations: true,
        always_on_top: false,
        skip_taskbar: true,
    },
    WindowSpec {
        w: "W9",
        label: "panel-pairing",
        title: "AI-Bridge 配对",
        width: 520.0,
        height: 620.0,
        resizable: false,
        decorations: true,
        always_on_top: false,
        skip_taskbar: true,
    },
    WindowSpec {
        w: "W10",
        label: "panel-settings",
        title: "AI-Bridge 设置",
        width: 640.0,
        height: 560.0,
        resizable: true,
        decorations: true,
        always_on_top: false,
        skip_taskbar: true,
    },
    WindowSpec {
        w: "W11",
        label: "wizard-init",
        title: "AI-Bridge 初始化向导",
        width: 560.0,
        height: 440.0,
        resizable: false,
        decorations: true,
        always_on_top: false,
        skip_taskbar: false,
    },
    WindowSpec {
        w: "W12",
        label: "wizard-pairing",
        title: "AI-Bridge 配对向导",
        width: 560.0,
        height: 560.0,
        resizable: false,
        decorations: true,
        always_on_top: false,
        skip_taskbar: false,
    },
    WindowSpec {
        w: "W13",
        label: "wizard-restore",
        title: "AI-Bridge 恢复向导",
        width: 560.0,
        height: 480.0,
        resizable: false,
        decorations: true,
        always_on_top: false,
        skip_taskbar: false,
    },
    WindowSpec {
        w: "W14",
        label: "wizard-drill",
        title: "AI-Bridge 恢复演练",
        width: 560.0,
        height: 460.0,
        resizable: false,
        decorations: true,
        always_on_top: false,
        skip_taskbar: false,
    },
];

pub fn find(label: &str) -> Option<&'static WindowSpec> {
    WINDOW_SPECS.iter().find(|spec| spec.label == label)
}

pub fn ensure_open(app: &AppHandle, label: &str) -> Result<(), String> {
    let spec = find(label).ok_or_else(|| format!("未声明的窗口标签：{label}"))?;

    if let Some(window) = app.get_webview_window(label) {
        window.show().map_err(|error| error.to_string())?;
        window.set_focus().map_err(|error| error.to_string())?;
        return Ok(());
    }

    build(app, spec)
}

pub fn hide(app: &AppHandle, label: &str) -> Result<(), String> {
    let window = app
        .get_webview_window(label)
        .ok_or_else(|| format!("窗口不存在：{label}"))?;
    window.hide().map_err(|error| error.to_string())
}

pub fn destroy(app: &AppHandle, label: &str) -> Result<(), String> {
    match app.get_webview_window(label) {
        Some(window) => window.destroy().map_err(|error| error.to_string()),
        None => Ok(()),
    }
}

pub fn toggle(app: &AppHandle, label: &str) -> Result<(), String> {
    match app.get_webview_window(label) {
        Some(window) => {
            let visible = window.is_visible().map_err(|error| error.to_string())?;
            if visible {
                window.hide().map_err(|error| error.to_string())
            } else {
                window.show().map_err(|error| error.to_string())
            }
        }
        None => ensure_open(app, label),
    }
}

fn build(app: &AppHandle, spec: &WindowSpec) -> Result<(), String> {
    let url = WebviewUrl::App(format!("index.html?window={}", spec.label).into());

    WebviewWindowBuilder::new(app, spec.label, url)
        .title(spec.title)
        .inner_size(spec.width, spec.height)
        .resizable(spec.resizable)
        .decorations(spec.decorations)
        .always_on_top(spec.always_on_top)
        .skip_taskbar(spec.skip_taskbar)
        .center()
        .build()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn open_window(app: AppHandle, label: String) -> Result<(), String> {
    ensure_open(&app, &label)
}

/// 关闭窗口：向导（`wizard-*`）一次性销毁，面板与候选条隐藏保留实例。
#[tauri::command]
pub fn close_window(app: AppHandle, label: String) -> Result<(), String> {
    if label.starts_with(WIZARD_PREFIX) {
        destroy(&app, &label)
    } else {
        hide(&app, &label)
    }
}

#[tauri::command]
pub fn toggle_window(app: AppHandle, label: String) -> Result<(), String> {
    toggle(&app, &label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs_are_unique_and_cover_all_windows() {
        assert_eq!(WINDOW_SPECS.len(), 13);
        for (index, spec) in WINDOW_SPECS.iter().enumerate() {
            for other in &WINDOW_SPECS[index + 1..] {
                assert_ne!(spec.label, other.label, "窗口 label 重复：{}", spec.label);
            }
            assert!(spec.width > 0.0 && spec.height > 0.0);
            assert!(spec.title.contains("AI-Bridge"), "窗口标题缺少产品名：{}", spec.title);
        }
    }

    #[test]
    fn group_prefixes_match_the_documented_groups() {
        let panels = WINDOW_SPECS
            .iter()
            .filter(|spec| spec.label.starts_with(PANEL_PREFIX))
            .count();
        let wizards = WINDOW_SPECS
            .iter()
            .filter(|spec| spec.label.starts_with(WIZARD_PREFIX))
            .count();
        assert_eq!(panels, 8, "面板应为 W3–W10 共 8 个");
        assert_eq!(wizards, 4, "向导应为 W11–W14 共 4 个");
        assert_eq!(
            WINDOW_SPECS
                .iter()
                .filter(|spec| !spec.label.starts_with(PANEL_PREFIX)
                    && !spec.label.starts_with(WIZARD_PREFIX))
                .count(),
            1,
            "非面板非向导的窗口只有候选条"
        );
    }

    #[test]
    fn unknown_label_is_rejected_without_panic() {
        assert!(find("panel-does-not-exist").is_none());
        assert!(find(CANDIDATE_LABEL).is_some());
        assert_eq!(find(CANDIDATE_LABEL).map(|spec| spec.w), Some("W2"));
    }
}
