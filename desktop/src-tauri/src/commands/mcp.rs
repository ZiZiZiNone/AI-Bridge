//! C1 MCP 服务的宿主接线（stdio + 127.0.0.1 Streamable HTTP，宿主进程内嵌）。
//!
//! 只做装配：`ab-mcp::McpService` 用同一份 broker / 会话查询组装（工具面规则在 core）；
//! stdio 通道由 `main.rs --mcp-stdio` 分流（MCP 客户端以子进程启动本程序），
//! HTTP 通道由 `mcp_start_http` 在宿主进程内起监听（**仅 127.0.0.1**，端口由调用方给）。
//!
//! 待确认：HTTP 端口的默认值与发现方式（客户端配置约定）docs/ 未定稿，
//! 因此不自动启动、不设默认端口 —— 端口由 UI / CLI 显式给出。

use std::sync::Arc;

use ab_mcp::{McpService, SessionLookup};
use ab_state::{Clock, SystemClock};

use super::CmdResult;
use crate::host::HostState;
use crate::native::dpapi::CngEntropy;

/// 组装一个 MCP 服务实例（每连接 / 每客户端一个）。
pub fn make_service(services: &HostState) -> McpService {
    McpService::new(
        services.broker.clone(),
        services.state.clone(),
        Arc::new(SystemClock),
        Arc::new(services.entropy),
        crate::host::DESKTOP_ACTOR,
    )
}

/// MCP 通道状态（设置面板展示）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct McpStatusView {
    pub stdio_entry: &'static str,
    pub http_running: bool,
    pub http_port: Option<u16>,
}

/// stdio 入口（`main.rs --mcp-stdio`）：独立进程内跑 MCP server 直到对端关闭。
pub fn run_stdio_entry() -> Result<(), String> {
    let services = HostState::build(crate::host::HostLayout {
        root: crate::host::HostLayout::default_root(),
    });
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    runtime
        .block_on(ab_mcp::serve_stdio(make_service(&services)))
        .map_err(|error| error.to_string())
}

/// 在宿主进程内启动 127.0.0.1 Streamable HTTP（每会话一个服务实例）。
pub fn start_http(services: &HostState, port: u16) -> CmdResult<McpStatusView> {
    {
        let mut running = services
            .mcp_http
            .lock()
            .map_err(|_| "MCP 状态互斥锁中毒".to_string())?;
        if running.is_some() {
            return Err("MCP HTTP 服务已在运行".into());
        }
        let probe = std::net::TcpListener::bind(("127.0.0.1", port))
            .map_err(|error| format!("MCP 回环端口绑定失败：{error}"))?;
        drop(probe);
        *running = Some(port);
    }

    let broker = services.broker.clone();
    let sessions: Arc<dyn SessionLookup> = services.state.clone();
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let entropy: Arc<dyn ab_crypto::Entropy> = Arc::new(CngEntropy);
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build();
        let Ok(runtime) = runtime else {
            return;
        };
        let _ = runtime.block_on(async {
            ab_mcp::serve_http_loopback(
                move || {
                    Ok(McpService::new(
                        broker.clone(),
                        sessions.clone(),
                        clock.clone(),
                        entropy.clone(),
                        crate::host::DESKTOP_ACTOR,
                    ))
                },
                port,
            )
            .await
        });
    });

    status(services)
}

/// 通道状态。
pub fn status(services: &HostState) -> CmdResult<McpStatusView> {
    let running = services
        .mcp_http
        .lock()
        .map_err(|_| "MCP 状态互斥锁中毒".to_string())?;
    Ok(McpStatusView {
        stdio_entry: "--mcp-stdio",
        http_running: running.is_some(),
        http_port: *running,
    })
}

/* ---------------- Tauri 命令包装（只转发） ---------------- */

#[tauri::command]
pub fn mcp_start_http(
    state: tauri::State<'_, HostState>,
    port: u16,
) -> CmdResult<McpStatusView> {
    start_http(&state, port)
}

#[tauri::command]
pub fn mcp_status(state: tauri::State<'_, HostState>) -> CmdResult<McpStatusView> {
    status(&state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{HostLayout, HostSignatureVerifier};
    use ab_audit::InMemoryAuditSink;
    use ab_mcp::{AcquireParams, McpScope, McpTarget};
    use std::sync::Arc;

    #[test]
    fn make_service_shares_broker_and_serves_tools() {
        let root = crate::host::test_root("aibridge-mcp-test");
        let _ = std::fs::remove_dir_all(&root);
        let services = HostState::build_with(
            "pc-test",
            HostLayout { root },
            HostSignatureVerifier::new(None, None),
            Arc::new(InMemoryAuditSink::new()),
        );

        // 工具面 → broker：无真会话时 `acquire_session` 如实报错（业务判断在 core）。
        let service = make_service(&services);
        let error = service
            .acquire_session(&AcquireParams {
                target: McpTarget {
                    kind: "web".into(),
                    domain: Some("github.com".into()),
                    app_id: None,
                },
                scope: McpScope {
                    domain_allowlist: vec!["github.com".into()],
                    read_only: true,
                    single_use: false,
                },
                ttl_secs: None,
                account_ref: None,
            })
            .unwrap_err();
        assert!(error.to_string().contains("没有可用会话"), "实际：{error}");
    }

    #[test]
    fn http_status_is_fair_and_refuses_double_start() {
        let root = crate::host::test_root("aibridge-mcp-http-test");
        let _ = std::fs::remove_dir_all(&root);
        let services = HostState::build_with(
            "pc-test",
            HostLayout { root },
            HostSignatureVerifier::new(None, None),
            Arc::new(InMemoryAuditSink::new()),
        );

        assert!(!status(&services).expect("状态失败").http_running);
        // 用高位端口真实起一次（127.0.0.1；测试结束随进程回收）。
        let view = start_http(&services, 47_321).expect("启动失败");
        assert!(view.http_running);
        assert_eq!(view.http_port, Some(47_321));
        assert_eq!(
            start_http(&services, 47_322).unwrap_err(),
            "MCP HTTP 服务已在运行"
        );
    }
}
