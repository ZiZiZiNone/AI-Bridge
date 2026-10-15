//! AI-Bridge Windows 桌面端可执行入口（Tauri 2 宿主层，薄入口）。

use std::sync::Arc;

pub mod bridge;
pub mod commands;
pub mod host;
pub mod native;
pub mod ui;

const NATIVE_MESSAGING_FLAG: &str = "--native-messaging-host";

fn main() {
    if std::env::args().any(|arg| arg == NATIVE_MESSAGING_FLAG) {
        if let Err(error) = bridge::messaging_host::run_stdio() {
            eprintln!("AI-Bridge native messaging host 退出：{error}");
            std::process::exit(1);
        }
        return;
    }

    if let Err(error) = run_desktop() {
        eprintln!("AI-Bridge 桌面端启动失败：{error}");
        std::process::exit(1);
    }
}

fn run_desktop() -> Result<(), String> {
    let host_state = Arc::new(host::HostState::try_new().map_err(|e| e.to_string())?);
    let mcp_controller = Arc::new(commands::mcp::McpController::new());

    tauri::Builder::default()
        .manage(host_state)
        .manage(mcp_controller)
        .invoke_handler(tauri::generate_handler![
            ui::window::open_window,
            ui::window::close_window,
            ui::window::toggle_window,
            commands::audit::audit_query,
            commands::audit::audit_count,
            commands::backup::backup_list_snapshots,
            commands::backup::backup_create_snapshot,
            commands::backup::backup_rollback_snapshot,
            commands::candidate::candidate_list_sessions,
            commands::converters::converters_validate_path,
            commands::converters::converters_verify_manifest,
            commands::library::library_is_unlocked,
            commands::library::library_unlock,
            commands::library::library_lock,
            commands::library::library_list_entries,
            commands::library::library_export_envelope,
            commands::mcp::mcp_start,
            commands::mcp::mcp_stop,
            commands::mcp::mcp_status,
            commands::pairing::pairing_capture_mobile_envelope,
            commands::pairing::pairing_export_envelope,
            commands::sessions::sessions_list,
            commands::sessions::sessions_issue,
            commands::sessions::sessions_renew,
            commands::sessions::sessions_release,
            commands::sessions::sessions_revoke,
            commands::sessions::sessions_revoke_all,
            commands::sessions::sessions_sweep,
            commands::sessions::sessions_list_pending_approvals,
            commands::sessions::sessions_approve_write,
            commands::sessions::sessions_deny_write,
            commands::state::state_list_sessions,
            commands::state::state_capture_browser_envelope,
            commands::state::state_ingest_standard_access,
        ])
        .setup(|app| {
            ui::tray::install(app.handle())?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .map_err(|error| error.to_string())
}
