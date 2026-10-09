//! 时间注入点。健康倒计时与快照时间戳全部取自本 trait，测试用固定时钟保证确定性
//! （仓库无全局单例，时钟由宿主构造后注入）。

use std::time::{SystemTime, UNIX_EPOCH};

/// 当前时间来源：Unix 纪元秒（UTC）。
pub trait Clock: Send + Sync {
    /// 当前 Unix 纪元秒（UTC）。
    fn now_unix_secs(&self) -> i64;
}

/// 生产实现：系统时钟。系统时间早于纪元时返回 0（不做回拨补偿）。
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_unix_secs(&self) -> i64 {
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(elapsed) => elapsed.as_secs().min(i64::MAX as u64) as i64,
            Err(_) => 0,
        }
    }
}
