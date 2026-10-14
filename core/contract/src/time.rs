//! RFC3339(UTC) ↔ Unix 纪元秒：契约 `date-time` 字段（`issued_at` / `expires_at`）的唯一编解码实现。
//!
//! 仓库依赖仅 serde / serde_json，`std` 无日历换算能力；此前三份 crate 各持一份最小实现
//! （ab-state 解析、ab-broker 格式化），按「同一业务规则只实现一份」上收到契约层。
//!
//! 解析支持 `YYYY-MM-DDTHH:MM:SS[.fraction](Z|±HH:MM)`：小数秒丢弃（秒级精度足够），
//! 闰秒（秒 = 60）不接受，非法输入返回 `None`（不 panic）。
//! 格式化输出 `YYYY-MM-DDTHH:MM:SSZ`。换算用 Howard Hinnant 的 civil 算法（纪元 1970-01-01）。

/// 解析为 Unix 纪元秒（UTC）；非法输入返回 `None`。
pub fn parse_rfc3339_secs(input: &str) -> Option<i64> {
    let bytes = input.as_bytes();
    if bytes.len() < 20 {
        return None;
    }

    let year = digits(input, 0, 4)?;
    if bytes[4] != b'-' {
        return None;
    }
    let month = digits(input, 5, 2)?;
    if bytes[7] != b'-' {
        return None;
    }
    let day = digits(input, 8, 2)?;
    if bytes[10] != b'T' && bytes[10] != b't' {
        return None;
    }
    let hour = digits(input, 11, 2)?;
    if bytes[13] != b':' {
        return None;
    }
    let minute = digits(input, 14, 2)?;
    if bytes[16] != b':' {
        return None;
    }
    let second = digits(input, 17, 2)?;

    let mut idx = 19;
    if idx < bytes.len() && bytes[idx] == b'.' {
        idx += 1;
        let start = idx;
        while idx < bytes.len() && bytes[idx].is_ascii_digit() {
            idx += 1;
        }
        if idx == start {
            return None;
        }
    }

    let offset_secs = if idx + 6 <= bytes.len() && (bytes[idx] == b'+' || bytes[idx] == b'-') {
        let negative = bytes[idx] == b'-';
        let off_hour = digits(input, idx + 1, 2)?;
        if bytes[idx + 3] != b':' {
            return None;
        }
        let off_minute = digits(input, idx + 4, 2)?;
        if off_hour > 23 || off_minute > 59 {
            return None;
        }
        idx += 6;
        let secs = off_hour * 3600 + off_minute * 60;
        if negative {
            -secs
        } else {
            secs
        }
    } else if idx < bytes.len() && (bytes[idx] == b'Z' || bytes[idx] == b'z') {
        idx += 1;
        0
    } else {
        return None;
    };

    if idx != bytes.len() {
        return None;
    }
    if !(1..=9999).contains(&year) || !(1..=12).contains(&month) {
        return None;
    }
    if !(1..=days_in_month(year, month)).contains(&day) {
        return None;
    }
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }

    Some(
        days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second
            - offset_secs,
    )
}

/// 纪元秒（UTC）→ RFC3339 秒级精度字符串。
pub fn format_rfc3339_secs(unix_secs: i64) -> String {
    let days = unix_secs.div_euclid(86_400);
    let rem = unix_secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// 按绝对字节区间取定长十进制；非数字或越界返回 `None`。
fn digits(input: &str, start: usize, len: usize) -> Option<i64> {
    let end = start.checked_add(len)?;
    let slice = input.get(start..end)?;
    if !slice.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    slice.parse::<i64>().ok()
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(year) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

/// 公历日期 → 纪元天数（Howard Hinnant 的 days_from_civil，纪元为 1970-01-01）。
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_shifted = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * month_shifted + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// 纪元天数 → 公历日期（days_from_civil 的逆）。
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::{format_rfc3339_secs, parse_rfc3339_secs};

    // 期望值由 Python 3.11（datetime.timezone）独立算出，与实现无关。
    #[test]
    fn parses_known_epochs() {
        let cases = [
            ("1970-01-01T00:00:00Z", 0),
            ("1969-12-31T23:59:59Z", -1),
            ("2000-02-29T23:59:59Z", 951_868_799),
            ("2024-02-29T12:34:56Z", 1_709_210_096),
            ("2100-03-01T00:00:00Z", 4_107_542_400),
            ("2026-01-01T08:00:00+08:00", 1_767_225_600),
            ("2026-10-09T00:00:00Z", 1_791_504_000),
            ("2026-10-09T08:00:00+08:00", 1_791_504_000),
            ("2026-10-09T00:00:00.250Z", 1_791_504_000),
            ("2026-10-09t00:00:00z", 1_791_504_000),
            ("2026-10-08T16:00:00-08:00", 1_791_504_000),
        ];
        for (input, expected) in cases {
            assert_eq!(parse_rfc3339_secs(input), Some(expected), "输入 {input}");
        }
    }

    #[test]
    fn rejects_malformed_input() {
        let cases = [
            "",
            "garbage",
            "2026-10-09T00:00:00",
            "2026-10-09 00:00:00Z",
            "2026-13-01T00:00:00Z",
            "2026-00-01T00:00:00Z",
            "2026-10-00T00:00:00Z",
            "2026-10-32T00:00:00Z",
            "2026-02-29T00:00:00Z",
            "2100-02-29T00:00:00Z",
            "2023-02-29T00:00:00Z",
            "2026-10-09T24:00:00Z",
            "2026-10-09T00:60:00Z",
            "2026-10-09T00:00:60Z",
            "2026-10-09T00:00:00+25:00",
            "2026-10-09T00:00:00+08:0",
            "2026-10-09T00:00:00.Z",
            "2026-10-09T00:00:00Z ",
            "2026-1-09T00:00:00Z",
            "0000-01-01T00:00:00Z",
        ];
        for input in cases {
            assert_eq!(parse_rfc3339_secs(input), None, "输入 {input}");
        }
    }

    #[test]
    fn formats_known_epochs() {
        let cases = [
            (0, "1970-01-01T00:00:00Z"),
            (-1, "1969-12-31T23:59:59Z"),
            (951_868_799, "2000-02-29T23:59:59Z"),
            (1_709_210_096, "2024-02-29T12:34:56Z"),
            (4_107_542_400, "2100-03-01T00:00:00Z"),
            (1_791_504_000, "2026-10-09T00:00:00Z"),
        ];
        for (secs, expected) in cases {
            assert_eq!(format_rfc3339_secs(secs), expected, "纪元秒 {secs}");
        }
    }

    #[test]
    fn format_parse_roundtrip() {
        for secs in [0_i64, -1, 951_868_799, 1_791_504_000, 4_107_542_400] {
            assert_eq!(parse_rfc3339_secs(&format_rfc3339_secs(secs)), Some(secs));
        }
    }
}
