//! AI-Bridge Windows 桌面端可执行入口（Tauri 2 宿主层，薄入口）。
//!
//! 分层（docs/架构与契约.md）：契约层 ← 核心层 ← 宿主层。
//! 本文件只做「装配 + 转发」：注册命令、装托盘、启动事件循环；
//! 无业务判断、无数据转换、无全局单例（依赖一律经参数注入）。

mod bridge;
mod native;
mod ui;

/// 浏览器扩展 native messaging 宿主的分流参数。
///
/// Chromium / Edge 的 native messaging 清单只能指向可执行文件、不能带参数，
/// 实现期注册为独立可执行文件；骨架期先用参数分流，避免出现第二份桥代码。
const NATIVE_MESSAGING_FLAG: &str = "--native-messaging-host";

fn main() {
    if std::env::args().any(|arg| arg == NATIVE_MESSAGING_FLAG) {
        if let Err(error) = bridge::messaging_host::run_stdio() {
            eprintln!("AI-Bridge native messaging host 退出：{error}");
            std::process::exit(1);
        }
        return;
    }

    run_desktop();
}

/// 启动桌面宿主：常驻托盘（W1），窗口按需创建（ui::window 的窗口清单）。
fn run_desktop() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            ui::window::open_window,
            ui::window::close_window,
            ui::window::toggle_window,
        ])
        .setup(|app| {
            // 托盘：图标三态（已锁 / 就绪 / 告警）+ 菜单（清单见 ui::tray::menu::MENU_ITEMS）。
            ui::tray::install(app.handle())?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("AI-Bridge 桌面端启动失败");
}
