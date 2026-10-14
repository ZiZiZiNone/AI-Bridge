//! 极简 RFC3339(UTC) 格式化（纪元秒 → `YYYY-MM-DDTHH:MM:SSZ`）。
//!
//! 与 `ab-state` 的解析侧对称；仓库依赖仅 serde，`std` 无日历换算能力，
//! 契约 `date-time` 字段（`issued_at` / `expires_at`）要输出，故此处自带最小实现。
//! 换算用 Howard Hinnant 的 civil_from_days（纪元 1970-01-01），与 `ab-state::time` 同源。

/// 纪元秒（UTC）→ RFC3339 秒级精度字符串。
pub(crate) fn format_rfc3339(unix_secs: i64) -> String {
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
    use super::format_rfc3339;

    // 期望值由 Python 3.11（datetime.timezone）独立算出，与实现无关。
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
            assert_eq!(format_rfc3339(secs), expected, "纪元秒 {secs}");
        }
    }
}
