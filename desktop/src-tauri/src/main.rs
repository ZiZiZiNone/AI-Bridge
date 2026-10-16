//! AI-Bridge Windows 桌面端可执行入口（Tauri 2 宿主层，薄入口）。
//!
//! 分层（docs/架构与契约.md）：契约层 ← 核心层 ← 宿主层。
//! 本文件只做「装配 + 转发」：注册命令、装托盘、分流进程角色、启动事件循环；
//! 无业务判断、无数据转换、无全局单例（依赖一律经参数注入，见 `host::HostState`）。

mod bridge;
mod commands;
mod host;
mod native;
mod ui;

use host::{HostLayout, HostState};
use native::protocol::ProtocolAction;

/// 浏览器扩展 native messaging 宿主的分流参数。
///
/// Chromium / Edge 的 native messaging 清单只能指向可执行文件、不能带参数，
/// 实现期注册为独立可执行文件；当前先用参数分流，避免出现第二份桥代码。
const NATIVE_MESSAGING_FLAG: &str = "--native-messaging-host";

/// MCP stdio 通道分流参数（C1.1：MCP 客户端以子进程方式启动本程序）。
const MCP_STDIO_FLAG: &str = "--mcp-stdio";

fn main() {
    if std::env::args().any(|arg| arg == NATIVE_MESSAGING_FLAG) {
        if let Err(error) = bridge::messaging_host::run_stdio() {
            eprintln!("AI-Bridge native messaging host 退出：{error}");
            std::process::exit(1);
        }
        return;
    }

    if std::env::args().any(|arg| arg == MCP_STDIO_FLAG) {
        if let Err(error) = commands::mcp::run_stdio_entry() {
            eprintln!("AI-Bridge MCP stdio 服务退出：{error}");
            std::process::exit(1);
        }
        return;
    }

    run_desktop();
}

/// 启动桌面宿主：常驻托盘（W1），窗口按需创建（ui::window 的窗口清单）。
fn run_desktop() {
    // D3：`aibridge://` 唤起参数（协议注册在 setup 内完成）。
    let uri_action = std::env::args().find_map(|arg| native::protocol::parse(&arg));
    let services = HostState::build(HostLayout {
        root: HostLayout::default_root(),
    });

    tauri::Builder::default()
        .manage(services)
        .invoke_handler(tauri::generate_handler![
            // W3 库面板 / W11 初始化（A1 / S1 / S3）
            commands::library::vault_create,
            commands::library::vault_unlock,
            commands::library::vault_lock,
            commands::library::vault_is_unlocked,
            commands::library::vault_list_entries,
            commands::library::vault_list_trash,
            commands::library::vault_get_entry,
            commands::library::vault_add_entry,
            commands::library::vault_update_entry,
            commands::library::vault_trash_entry,
            commands::library::vault_restore_entry,
            commands::library::vault_purge_trash,
            // W7 备份 / W13 恢复 / W14 演练（A6）
            commands::backup::backup_run,
            commands::backup::backup_status,
            commands::backup::backup_drill,
            commands::backup::backup_restore_with_password,
            commands::backup::backup_restore_with_recovery,
            // W4 状态清单（A2 / A3 / A7 / B1.3）
            commands::state::state_matrix,
            commands::state::state_health,
            commands::state::state_snapshots,
            commands::state::state_rollback,
            commands::state::state_capture,
            // W5 临时会话（C1–C5）
            commands::sessions::session_issue,
            commands::sessions::session_list,
            commands::sessions::session_renew,
            commands::sessions::session_release,
            commands::sessions::session_revoke,
            commands::sessions::session_revoke_all,
            commands::sessions::session_sweep,
            // W2 候选条数据源（B5 / B6.2 / A4.1）
            commands::candidate::candidate_data,
            // W8 转换器（B4.1）
            commands::converters::converter_list,
            commands::converters::converter_install,
            // W9 配对 / 设备（E1–E3 / A8）
            commands::pairing::pairing_offer,
            commands::pairing::pairing_review,
            commands::pairing::pairing_confirm,
            commands::pairing::pairing_devices,
            commands::pairing::pairing_unbind,
            commands::pairing::sync_transfer,
            commands::pairing::sync_export_pack,
            commands::pairing::sync_import_pack,
            // W6 审计（C3）
            commands::audit::audit_query,
            commands::audit::audit_count,
            // C1 MCP 通道（127.0.0.1 Streamable HTTP，进程内嵌）
            commands::mcp::mcp_start_http,
            commands::mcp::mcp_status,
            // 窗口管理（既有）
            ui::window::open_window,
            ui::window::close_window,
            ui::window::toggle_window,
        ])
        .setup(move |app| {
            // 托盘：图标三态（已锁 / 就绪 / 告警）+ 菜单（清单见 ui::tray::menu::MENU_ITEMS）。
            ui::tray::install(app.handle())?;

            // D3 协议：首次运行登记 `aibridge://`（HKCU，免管理员；失败不阻断启动）。
            if let Err(error) = native::protocol::register() {
                eprintln!("AI-Bridge 协议登记未完成：{error}");
            }

            // D3 反向唤起：本次启动带 URI 参数 → 直接落到对应动作。
            if let Some(action) = uri_action {
                dispatch_protocol(app.handle(), action);
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("AI-Bridge 桌面端启动失败");
}

/// `aibridge://` 动作 → 窗口 / 候选条（机械映射，不含业务判断）。
fn dispatch_protocol(app: &tauri::AppHandle, action: ProtocolAction) {
    match action {
        ProtocolAction::ShowCandidate => {
            if let Err(error) = ui::candidate::summon(app, ui::candidate::ShowAnchor::Pointer) {
                eprintln!("AI-Bridge 候选条打开失败：{error}");
            }
        }
        ProtocolAction::ShowPanel(short_name) => {
            if let Some(label) = ui::panels::label_of(&short_name) {
                if let Err(error) = ui::panels::open(app, label) {
                    eprintln!("AI-Bridge 面板 {label} 打开失败：{error}");
                }
            } else {
                eprintln!("AI-Bridge 未知面板短名：{short_name}");
            }
        }
        ProtocolAction::Pair => {
            if let Err(error) = ui::wizard::open(app, "wizard-pairing") {
                eprintln!("AI-Bridge 配对向导打开失败：{error}");
            }
        }
    }
}
