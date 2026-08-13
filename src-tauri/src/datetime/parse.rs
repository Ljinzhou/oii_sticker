//! 日期时间字符串解析与格式化。

use super::DateTime;

/// 解析 `YYYY-MM-DD`（时分归零）或 `YYYY-MM-DD HH:MM` / `YYYY-MM-DDTHH:MM[:SS]`。
/// 失败返回 None。
pub fn parse(s: &str) -> Option<DateTime> {
    let s = s.trim();
    let bytes = s.as_bytes();
    // 合法长度：10（YYYY-MM-DD）、16（+HH:MM）、19（+HH:MM:SS）。
    if !matches!(s.len(), 10 | 16 | 19) {
        return None;
    }
    // 校验日期分隔符。
    if bytes.get(4) != Some(&b'-') || bytes.get(7) != Some(&b'-') {
        return None;
    }
    let p = |a: usize, b: usize| -> Option<u32> {
        std::str::from_utf8(bytes.get(a..b)?).ok()?.parse().ok()
    };
    let year = p(0, 4)?;
    let month = p(5, 7)?;
    let day = p(8, 10)?;
    let (hour, minute) = if s.len() >= 16 {
        // 校验时间分隔符（空格或 T）与时:分冒号。
        if bytes.get(10) != Some(&b' ') && bytes.get(10) != Some(&b'T') {
            return None;
        }
        if bytes.get(13) != Some(&b':') {
            return None;
        }
        // 带秒格式：校验冒号与秒范围（秒值本身被忽略）。
        if s.len() == 19 {
            if bytes.get(16) != Some(&b':') {
                return None;
            }
            let sec: u32 = p(17, 19)?;
            if sec > 59 {
                return None;
            }
        }
        (p(11, 13)?, p(14, 16)?)
    } else {
        (0, 0)
    };
    DateTime::new(year, month, day, hour, minute)
}

/// 格式化：`YYYY-MM-DD`；`with_time` 为 true 时追加 ` HH:MM`。
pub fn format(dt: &DateTime, with_time: bool) -> String {
    if with_time {
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}",
            dt.year, dt.month, dt.day, dt.hour, dt.minute
        )
    } else {
        format!("{:04}-{:02}-{:02}", dt.year, dt.month, dt.day)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_date_only() {
        let dt = parse("2026-08-01").unwrap();
        assert_eq!(dt, DateTime::from_ymd(2026, 8, 1).unwrap());
    }

    #[test]
    fn parse_with_time() {
        let dt = parse("2026-08-01 12:30").unwrap();
        assert_eq!(dt, DateTime::new(2026, 8, 1, 12, 30).unwrap());
        let dt2 = parse("2026-08-01T12:30:00").unwrap();
        assert_eq!(dt2, DateTime::new(2026, 8, 1, 12, 30).unwrap());
    }

    #[test]
    fn parse_invalid() {
        assert!(parse("2026-13-01").is_none());
        assert!(parse("abcd-ef-gh").is_none());
        assert!(parse("").is_none());
        assert!(parse("2026-08-01 25:00").is_none());
    }

    #[test]
    fn format_roundtrip() {
        let dt = DateTime::new(2026, 8, 1, 9, 5).unwrap();
        assert_eq!(format(&dt, false), "2026-08-01");
        assert_eq!(format(&dt, true), "2026-08-01 09:05");
    }

    #[test]
    fn parse_rejects_trailing_garbage() {
        assert!(parse("2026-08-01x").is_none());
        assert!(parse("2026-08-01 12:30x").is_none());
        assert!(parse("2026-08-01T12:30:00x").is_none());
    }

    #[test]
    fn parse_rejects_wrong_separators() {
        assert!(parse("2026/08/01").is_none());
        assert!(parse("2026-08/01").is_none());
        assert!(parse("2026-08-01 12x30").is_none());
        // 秒超出范围。
        assert!(parse("2026-08-01T12:30:60").is_none());
    }
}
