//! 接入等级规则：**全仓唯一实现**（a 仅启动 / b 凭据填表 / c 会话注入）。
//!
//! 两个纯函数，无 IO、无状态：
//! - [`effective_level`]：清单声明等级与调用方请求等级取**能力更弱**者（不越权）；
//! - [`prune`]：按有效等级裁掉不该外流的数据段。

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

/// 有效等级 = 清单声明等级与请求等级中能力更弱的一个。
///
/// 例：清单声明 a（仅启动）的转换器，即使调用方请求 c，有效等级仍是 a。
pub fn effective_level(manifest_level: AccessLevel, requested: AccessLevel) -> AccessLevel {
    from_rank(rank(manifest_level).min(rank(requested)))
}

/// 按有效等级裁剪输出：
/// - **a**：只留 meta 与启动信息（`app_id` / `level` / `generated_at`），`credentials`、`sessions` 一律清空；
/// - **b**：只留 `credentials`（`sessions` 清空）；
/// - **c**：全量，含 `sessions`。
///
/// `meta` 仅来源自述（转换器版本、采集时间等展示信息），不含凭据数据，三个等级均保留。
/// 裁剪后把 `access.level` 写成有效等级，避免下游误读能力边界。
pub fn prune(access: &mut StandardAccess, effective: AccessLevel) {
    if matches!(effective, AccessLevel::A | AccessLevel::B) {
        access.sessions.clear();
    }
    if matches!(effective, AccessLevel::A) {
        access.credentials.clear();
    }
    access.level = effective;
}
