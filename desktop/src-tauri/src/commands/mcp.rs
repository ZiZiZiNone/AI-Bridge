//! MCP 本地回环服务控制、Origin 防护、AI Agent 零明文句柄代理与影子令牌网关接线（C5 / P1-5 / P1-11）。
//!
//! - 对应 vendored `rmcp-1.8.0` 的 `StreamableHttpServerConfig::allowed_origins`：
//!   显式收紧为本地回环来源白名单（禁止空列表默认放行任意跨源请求）；
//! - 绑定地址仅允许 `127.0.0.1` 或 `::1`；
//! - 运行状态标志 `running` 必须在 `TcpListener::bind` 真正成功后才置位；
//! - 同时支持 MCP 原生句柄调用（`agent_execute_request`）与传统非 MCP CLI 工具影子令牌调用（`agent_execute_shadow_token`）。

use std::net::{IpAddr, SocketAddr, TcpListener};
use std::sync::Mutex;

use ab_broker::{AgentHttpExecutor, AgentHttpRequest, AgentHttpResponse, IssuedSession};
use ab_contract::ScopeSpec;

use crate::host::HostState;

/// 回环 Streamable HTTP 允许的 Origin 前缀白名单（供 `rmcp` `allowed_origins` 配置及请求头复核）。
pub const LOOPBACK_ALLOWED_ORIGINS: &[&str] = &[
    "http://localhost",
    "http://127.0.0.1",
    "http://[::1]",
    "https://localhost",
    "https://127.0.0.1",
    "https://[::1]",
];

pub fn default_allowed_origins() -> Vec<String> {
    LOOPBACK_ALLOWED_ORIGINS
        .iter()
        .map(|s| (*s).to_string())
        .collect()
}

/// 校验 Origin 头是否严格属于本地回环（允许可选端口 `:port`，拒绝 `http://localhost.evil.example`）。
pub fn is_allowed_loopback_origin(origin: &str) -> bool {
    let trimmed = origin.trim();
    for &prefix in LOOPBACK_ALLOWED_ORIGINS {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            if rest.is_empty() {
                return true;
            }
            if let Some(port_str) = rest.strip_prefix(':') {
                if !port_str.is_empty() && port_str.parse::<u16>().is_ok() {
                    return true;
                }
            }
        }
    }
    false
}

pub fn validate_loopback_addr(addr: SocketAddr) -> Result<SocketAddr, String> {
    match addr.ip() {
        IpAddr::V4(ip) if ip.is_loopback() => Ok(addr),
        IpAddr::V6(ip) if ip.is_loopback() => Ok(addr),
        other => Err(format!("MCP HTTP 仅允许绑定本地回环地址，拒绝绑定：{other}")),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServerStatus {
    pub running: bool,
    pub bound_addr: Option<SocketAddr>,
    pub allowed_origins: Vec<String>,
}

pub struct McpController {
    status: Mutex<McpServerStatus>,
}

impl Default for McpController {
    fn default() -> Self {
        Self::new()
    }
}

impl McpController {
    pub fn new() -> Self {
        Self {
            status: Mutex::new(McpServerStatus {
                running: false,
                bound_addr: None,
                allowed_origins: default_allowed_origins(),
            }),
        }
    }

    pub fn bind_loopback(&self, requested: SocketAddr) -> Result<McpServerStatus, String> {
        let valid_addr = validate_loopback_addr(requested)?;
        let listener =
            TcpListener::bind(valid_addr).map_err(|e| format!("MCP 回环端口绑定失败：{e}"))?;
        let local_addr = listener
            .local_addr()
            .map_err(|e| format!("无法获取绑定地址：{e}"))?;
        drop(listener);

        let mut guard = self
            .status
            .lock()
            .map_err(|_| "MCP 状态互斥锁不可用".to_string())?;
        guard.running = true;
        guard.bound_addr = Some(local_addr);
        Ok(guard.clone())
    }

    pub fn stop(&self) -> Result<McpServerStatus, String> {
        let mut guard = self
            .status
            .lock()
            .map_err(|_| "MCP 状态互斥锁不可用".to_string())?;
        guard.running = false;
        guard.bound_addr = None;
        Ok(guard.clone())
    }

    pub fn status(&self) -> Result<McpServerStatus, String> {
        self.status
            .lock()
            .map(|g| g.clone())
            .map_err(|_| "MCP 状态互斥锁不可用".to_string())
    }

    /// MCP 工具：向本地 AI Agent 签发带设定时间（`ttl_seconds`）与 `ScopeSpec` 的临时会话句柄（含 `shadow_token`）。
    pub fn agent_issue_session(
        &self,
        host: &HostState,
        account_ref: &str,
        scope: ScopeSpec,
        ttl_seconds: u32,
    ) -> Result<IssuedSession, String> {
        if !self.status()?.running {
            return Err("MCP 服务未启动，拒绝签发 Agent 临时会话".into());
        }
        host.issue_temporary_session(account_ref, scope, ttl_seconds)
            .map_err(|e| e.to_string())
    }

    /// MCP 工具（Scheme A）：AI Agent 仅凭 `session_id` 发起受控 HTTPS 请求，宿主内部代填 Cookie/Token 并剥离响应 `Set-Cookie`。
    pub fn agent_execute_request(
        &self,
        host: &HostState,
        session_id: &str,
        request: AgentHttpRequest,
        executor: &dyn AgentHttpExecutor,
    ) -> Result<AgentHttpResponse, String> {
        if !self.status()?.running {
            return Err("MCP 服务未启动，拒绝执行 Agent 会话请求".into());
        }
        host.execute_agent_request(session_id, request, executor)
            .map_err(|e| e.to_string())
    }

    /// 本地环回影子令牌网关：支持传统 CLI 工具以 `Authorization: Bearer ab_tmp_...` 发起受控请求。
    pub fn agent_execute_shadow_token(
        &self,
        host: &HostState,
        shadow_token_or_bearer: &str,
        request: AgentHttpRequest,
        executor: &dyn AgentHttpExecutor,
    ) -> Result<AgentHttpResponse, String> {
        if !self.status()?.running {
            return Err("MCP 服务未启动，拒绝执行影子令牌请求".into());
        }
        host.execute_shadow_token_request(shadow_token_or_bearer, request, executor)
            .map_err(|e| e.to_string())
    }

    /// MCP 工具：主动收回指定临时会话。
    pub fn agent_revoke_session(&self, host: &HostState, session_id: &str) -> Result<(), String> {
        host.revoke_temporary_session(session_id)
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowed_origins_accepts_loopback_and_rejects_cross_origin() {
        assert!(!default_allowed_origins().is_empty());
        assert!(is_allowed_loopback_origin("http://127.0.0.1:8765"));
        assert!(is_allowed_loopback_origin("http://localhost:3000"));
        assert!(is_allowed_loopback_origin("http://[::1]:9000"));
        assert!(!is_allowed_loopback_origin("http://localhost.evil.example"));
        assert!(!is_allowed_loopback_origin("https://evil.example"));
        assert!(!is_allowed_loopback_origin("null"));
    }

    #[test]
    fn mcp_controller_rejects_non_loopback_and_keeps_running_false_on_failure() {
        let ctl = McpController::new();
        let bad_addr: SocketAddr = "0.0.0.0:0".parse().unwrap();
        assert!(ctl.bind_loopback(bad_addr).is_err());
        assert!(!ctl.status().unwrap().running);

        let ok_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        if let Ok(status) = ctl.bind_loopback(ok_addr) {
            assert!(status.running);
            assert!(status.bound_addr.is_some());
            let stopped = ctl.stop().unwrap();
            assert!(!stopped.running);
        }
    }
}
