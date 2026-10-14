//! AI-Bridge Windows 桌面端可执行入口（Tauri 2 宿主层，薄入口）。

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
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            ui::window::open_window,
            ui::window::close_window,
            ui::window::toggle_window,
        ])
        .setup(|app| {
            ui::tray::install(app.handle())?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .map_err(|error| error.to_string())
}
