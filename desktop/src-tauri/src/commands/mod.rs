//! Tauri 命令层（薄入口）：只做**参数搬运与错误映射**，业务规则一律在 core（ab-*）。
//!
//! 结构约定（极致解耦判据：搬进单测不改内部代码即可工作）：
//! - 每个面板一个子模块；模块内「纯函数」（`unlock` / `backup` / …）只接收
//!   `&HostState` 与结构化参数，返回结构化结果 —— 不碰 Tauri 对象，单测直接调用；
//! - `#[tauri::command]` 包装函数只做 `tauri::State` 取服务 + 转发（同名加面板前缀）；
//! - 错误映射集中在这里：core 错误的 `Display` 已按 F7 脱敏，命令层直接透出文本，
//!   绝不携带口令 / 密钥 / cookie / 句柄 / 完整路径。
//!
//! core 缺 API 的功能（A5 生成器、A1.2 检索、A1.3 体检、A6.6 滚动保留等）不在本层
//! 补第二份规则，见 desktop/README.md「core 缺口清单」。

pub mod audit;
pub mod backup;
pub mod candidate;
pub mod converters;
pub mod library;
pub mod mcp;
pub mod pairing;
pub mod sessions;
pub mod state;

use ab_audit::{AuditAction, AuditEvent, AuditResult};
use serde::{Deserialize, Serialize};

use crate::host::HostState;

/// 命令返回口径。
pub type CmdResult<T> = Result<T, String>;

/// 审计馈给（C3.1 / A2.3 / A6.7）：**fail-closed** —— 写账失败则返回错误，
/// 调用方（敏感操作）必须中止，禁止吞错。
pub fn record(
    services: &HostState,
    action: AuditAction,
    result: AuditResult,
    account_ref: Option<&str>,
    domain: Option<&str>,
    detail: &[(&str, &str)],
) -> CmdResult<()> {
    let mut event = AuditEvent::new(
        crate::host::DESKTOP_ACTOR,
        now_rfc3339(),
        action,
        result,
    );
    event.account_ref = account_ref.map(str::to_string);
    event.domain = domain.map(str::to_string);
    for (key, value) in detail {
        event.detail.insert((*key).to_string(), (*value).to_string());
    }
    services
        .audit
        .record_or_fail(event)
        .map_err(|error| error.to_string())
}

/// 当前时刻（Unix 纪元秒）。
pub fn now_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

/// 当前时刻（RFC 3339）。
pub fn now_rfc3339() -> String {
    ab_contract::time::format_rfc3339_secs(now_unix_secs())
}

/// 审计筛选 DTO（`ab_audit::AuditFilter` 不带 serde，命令面用本结构搬运）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AuditFilterDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub who: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<AuditAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<AuditResult>,
}

impl From<&AuditFilterDto> for ab_audit::AuditFilter {
    fn from(dto: &AuditFilterDto) -> Self {
        ab_audit::AuditFilter {
            who: dto.who.clone(),
            account_ref: dto.account_ref.clone(),
            domain: dto.domain.clone(),
            action: dto.action,
            result: dto.result,
        }
    }
}
