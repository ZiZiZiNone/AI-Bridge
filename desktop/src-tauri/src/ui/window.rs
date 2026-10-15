//! 窗口管理骨架：窗口清单（唯一事实源）+ 打开 / 隐藏 / 销毁 / 切换。
//!
//! 契约：
//! - 每个 Tauri 窗口的 `label` 与 `desktop/src/ui/window-registry.ts` 的 `WINDOW_VIEWS` 键一一对应；
//! - 窗口只加载 `index.html?window=<label>`，由标签决定挂载哪个视图（薄入口）；
//! - 尺寸 / 形态只在本文件声明，前端不重复声明（同一规则全仓只写一份）；
//! - 面板「用完即关」（隐藏保留 WebView），向导「用完即焚」（销毁）。
//!
//! 未验证：本机无 cargo 工具链，Tauri 2 API 形态（WebviewWindowBuilder 链式调用）未经编译验证。

use serde::Serialize;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// 候选条窗口 label（热键唤起，见 ui::candidate）。
pub const CANDIDATE_LABEL: &str = "candidate";
/// 面板窗口 label 前缀（W3–W10）。
pub const PANEL_PREFIX: &str = "panel-";
/// 向导窗口 label 前缀（W11–W14）。
pub const WIZARD_PREFIX: &str = "wizard-";

/// 单个窗口的声明（纯数据，便于单测与前端登记表比对）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct WindowSpec {
    /// 功能点全景 §3.1 的界面编号（W2–W14）
    pub w: &'static str,
    /// Tauri 窗口 label，同时决定前端挂载哪个视图
    pub label: &'static str,
    /// 窗口标题（显示名一律 AI-Bridge）
    pub title: &'static str,
    pub width: f64,
    pub height: f64,
    pub resizable: bool,
    pub decorations: bool,
    pub always_on_top: bool,
    pub skip_taskbar: bool,
}

/// 窗口清单（唯一事实源）：13 个窗口 = 候选条 1（W2）+ 面板 8（W3–W10）+ 向导 4（W11–W14）。
/// 托盘（W1）与通知（W15）不是窗口，故不在本表（见 ui::tray）。
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

/// 按 label 取窗口声明；未声明返回 None（调用方负责转成错误，不 panic）。
pub fn find(label: &str) -> Option<&'static WindowSpec> {
    WINDOW_SPECS.iter().find(|spec| spec.label == label)
}

/// 打开窗口；已存在则前置（不重复创建）。
pub fn ensure_open(app: &AppHandle, label: &str) -> Result<(), String> {
    let spec = find(label).ok_or_else(|| format!("未声明的窗口标签：{label}"))?;

    if let Some(window) = app.get_webview_window(label) {
        window.show().map_err(|error| error.to_string())?;
        window.set_focus().map_err(|error| error.to_string())?;
        return Ok(());
    }

    build(app, spec)
}

/// 隐藏窗口（面板「用完即关」：隐藏保留 WebView，下次打开更快）。
pub fn hide(app: &AppHandle, label: &str) -> Result<(), String> {
    let window = app
        .get_webview_window(label)
        .ok_or_else(|| format!("窗口不存在：{label}"))?;
    window.hide().map_err(|error| error.to_string())
}

/// 销毁窗口（向导「用完即焚」）。窗口不存在视为已完成。
pub fn destroy(app: &AppHandle, label: &str) -> Result<(), String> {
    match app.get_webview_window(label) {
        Some(window) => window.destroy().map_err(|error| error.to_string()),
        None => Ok(()),
    }
}

/// 切换显示（候选条热键 B6.1 用）。
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

/// 真正创建窗口：URL 里带 label，前端据此挂载视图（见 src/ui/window-registry.ts）。
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

/* ---------------- tauri commands：UI → 宿主（结构化数据进出，无业务规则） ---------------- */

#[tauri::command]
pub fn open_window(app: AppHandle, label: String) -> Result<(), String> {
    ensure_open(&app, &label)
}

#[tauri::command]
pub fn close_window(app: AppHandle, label: String) -> Result<(), String> {
    destroy(&app, &label)
}

#[tauri::command]
pub fn toggle_window(app: AppHandle, label: String) -> Result<(), String> {
    toggle(&app, &label)
}

#[cfg(test)]
mod tests {
    // 窗口清单是纯数据：不启任何宿主即可校验（极致解耦判据：搬进单测不改内部代码）。
    // 未验证：本机无 cargo，未运行。
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
