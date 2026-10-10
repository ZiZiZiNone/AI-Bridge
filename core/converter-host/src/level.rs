//! 接入等级规则（a 仅启动 / b 凭据填表 / c 会话注入），全仓唯一实现。

use ab_contract::{AccessLevel, StandardAccess};

/// 等级能力序：数值越大能力越强。
fn rank(level: AccessLevel) -> u8 {
    match level {
        AccessLevel::A => 0,
        AccessLevel::B => 1,
        AccessLevel::C => 2,
    }
}

fn from_rank(value: u8) -> AccessLevel {
    match value {
        0 => AccessLevel::A,
        1 => AccessLevel::B,
        _ => AccessLevel::C,
    }
}

/// 清单能力与请求取更弱者：声明 a 的转换器，请求 c 也只能按 a 放行。
pub fn effective_level(manifest_level: AccessLevel, requested: AccessLevel) -> AccessLevel {
    from_rank(rank(manifest_level).min(rank(requested)))
}

/// a 清空 credentials+sessions，b 清空 sessions，c 全量；meta 是来源自述，三级均留。
/// 裁剪后写回 `access.level`，避免下游误读能力边界。
pub fn prune(access: &mut StandardAccess, effective: AccessLevel) {
    if matches!(effective, AccessLevel::A | AccessLevel::B) {
        access.sessions.clear();
    }
    if matches!(effective, AccessLevel::A) {
        access.credentials.clear();
    }
    access.level = effective;
}
