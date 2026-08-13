//! 重复提醒规则：解析、下次触发时间计算、人类可读描述。
//!
//! 规则字符串格式（`remind_rule` 字段）：
//! - `daily`                   → 每天
//! - `weekly`                  → 每周（兼容旧格式，等价 `weekly:mon`）
//! - `weekly:mon,wed,fri`      → 每周指定几天（mon/tue/wed/thu/fri/sat/sun）
//! - `interval:N`              → 每 N 天（N ≥ 1）
//! - `monthly:N`               → 每月 N 号（N ≥ 1；超过当月天数时取当月最后一天）
//! - `yearly:M-D`              → 每年 M 月 D 日
//!
//! `next_occurrence(rule, from)` 返回**严格晚于** `from` 的下一次触发时间
//! （时间部分保持 `from` 的时:分；`from` 当天如果规则命中则取下一周期）。

use super::{days_in_month, DateTime};

/// 解析后的重复规则。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepeatRule {
    /// 每天。
    Daily,
    /// 每周指定几天（0 = 周日 – 6 = 周六）。
    Weekly(Vec<u32>),
    /// 每 N 天。
    IntervalDays(u32),
    /// 每月 N 号。
    Monthly(u32),
    /// 每年 M 月 D 日。
    Yearly(u32, u32),
}

/// 星期名（小写三字母）。
const WEEKDAYS: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];

/// 解析重复规则字符串；空串或非法返回 None。
pub fn parse_rule(s: &str) -> Option<RepeatRule> {
    let s = s.trim().to_lowercase();
    if s.is_empty() {
        return None;
    }
    let (name, args) = match s.split_once(':') {
        Some((n, a)) => (n, Some(a)),
        None => (s.as_str(), None),
    };
    match name {
        "daily" => Some(RepeatRule::Daily),
        "weekly" => {
            let days = match args {
                Some(a) => {
                    let mut v: Vec<u32> = a
                        .split(',')
                        .filter_map(|d| {
                            WEEKDAYS
                                .iter()
                                .position(|w| *w == d.trim())
                                .map(|i| i as u32)
                        })
                        .collect();
                    v.sort_unstable();
                    v.dedup();
                    if v.is_empty() {
                        return None;
                    }
                    v
                }
                None => vec![1], // 兼容旧格式：默认周一
            };
            Some(RepeatRule::Weekly(days))
        }
        "interval" => {
            let n: u32 = args?.trim().parse().ok()?;
            (n >= 1).then_some(RepeatRule::IntervalDays(n))
        }
        "monthly" => {
            let d: u32 = args?.trim().parse().ok()?;
            (d >= 1 && d <= 31).then_some(RepeatRule::Monthly(d))
        }
        "yearly" => {
            let (m, d) = args?.split_once('-')?;
            let m: u32 = m.trim().parse().ok()?;
            let d: u32 = d.trim().parse().ok()?;
            (m >= 1 && m <= 12 && d >= 1 && d <= 31).then_some(RepeatRule::Yearly(m, d))
        }
        _ => None,
    }
}

/// 计算 `from` 之后（严格晚于）的下一次触发时间。
///
/// 时间部分保持 `from` 的时:分。
pub fn next_occurrence(rule: &RepeatRule, from: DateTime) -> DateTime {
    let time = (from.hour, from.minute);
    let make = |year: u32, month: u32, day: u32| -> DateTime {
        DateTime::new(year, month, day, time.0, time.1).unwrap_or(from)
    };
    match rule {
        RepeatRule::Daily => from.add_days(1),
        RepeatRule::Weekly(days) => {
            // 从明天开始找最近命中的星期几。
            let mut cand = from.add_days(1);
            for _ in 0..8 {
                if days.contains(&cand.weekday()) {
                    return cand;
                }
                cand = cand.add_days(1);
            }
            from.add_days(7) // 理论上不可达，兜底
        }
        RepeatRule::IntervalDays(n) => from.add_days(*n as i64),
        RepeatRule::Monthly(day) => {
            let day = *day;
            // 本月候选：若 from 在本月 day 之前（且不同天）→ 本月触发。
            let this_month = make(
                from.year,
                from.month,
                day.min(days_in_month(from.year, from.month)),
            );
            if this_month > from {
                return this_month;
            }
            // 否则从下个月起。
            let y = from.year + if from.month == 12 { 1 } else { 0 };
            let m = if from.month == 12 { 1 } else { from.month + 1 };
            make(y, m, day.min(days_in_month(y, m)))
        }
        RepeatRule::Yearly(m, d) => {
            let m = *m;
            let d = *d;
            let mut y = from.year;
            // 若今年该日期已过（或就是今天），推到明年。
            let this_year = make(y, m, d.min(days_in_month(y, m)));
            if this_year <= from {
                y += 1;
            }
            make(y, m, d.min(days_in_month(y, m)))
        }
    }
}

/// 人类可读描述（用于主控台显示）。
pub fn describe(rule: &RepeatRule) -> String {
    match rule {
        RepeatRule::Daily => "每天".to_string(),
        RepeatRule::Weekly(days) => {
            let names: Vec<&str> = days
                .iter()
                .map(|d| match *d {
                    0 => "周日",
                    1 => "周一",
                    2 => "周二",
                    3 => "周三",
                    4 => "周四",
                    5 => "周五",
                    _ => "周六",
                })
                .collect();
            format!("每周{}", names.join("、"))
        }
        RepeatRule::IntervalDays(n) => format!("每 {n} 天"),
        RepeatRule::Monthly(d) => format!("每月 {d} 号"),
        RepeatRule::Yearly(m, d) => format!("每年 {m} 月 {d} 日"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datetime::DateTime;

    fn dt(y: u32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime {
        DateTime::new(y, mo, d, h, mi).unwrap()
    }

    // ---- 解析 ----

    #[test]
    fn parse_known_rules() {
        assert_eq!(parse_rule("daily"), Some(RepeatRule::Daily));
        assert_eq!(parse_rule("weekly"), Some(RepeatRule::Weekly(vec![1])));
        assert_eq!(
            parse_rule("weekly:mon,wed,fri"),
            Some(RepeatRule::Weekly(vec![1, 3, 5]))
        );
        assert_eq!(parse_rule("interval:3"), Some(RepeatRule::IntervalDays(3)));
        assert_eq!(parse_rule("monthly:15"), Some(RepeatRule::Monthly(15)));
        assert_eq!(parse_rule("yearly:12-25"), Some(RepeatRule::Yearly(12, 25)));
    }

    #[test]
    fn parse_invalid_rules() {
        assert!(parse_rule("").is_none());
        assert!(parse_rule("weekly:xyz").is_none());
        assert!(parse_rule("interval:0").is_none());
        assert!(parse_rule("monthly:0").is_none());
        assert!(parse_rule("yearly:13-01").is_none());
        assert!(parse_rule("foo").is_none());
    }

    // ---- next_occurrence ----

    #[test]
    fn next_daily() {
        let from = dt(2026, 8, 1, 12, 0);
        assert_eq!(next_occurrence(&RepeatRule::Daily, from), dt(2026, 8, 2, 12, 0));
    }

    #[test]
    fn next_interval() {
        let from = dt(2026, 8, 1, 9, 30);
        assert_eq!(
            next_occurrence(&RepeatRule::IntervalDays(3), from),
            dt(2026, 8, 4, 9, 30)
        );
    }

    #[test]
    fn next_weekly_picks_next_weekday() {
        // 2026-08-01 是周六；每周一、三 → 下一次是 08-03（周一）。
        let from = dt(2026, 8, 1, 10, 0);
        let rule = RepeatRule::Weekly(vec![1, 3]);
        assert_eq!(next_occurrence(&rule, from), dt(2026, 8, 3, 10, 0));

        // 周三 → 下一次周一（08-05 是周三，命中但 next 严格晚于 from；
        // 从 08-06 开始找 → 08-10 周一）。
        let from2 = dt(2026, 8, 5, 10, 0);
        assert_eq!(next_occurrence(&rule, from2), dt(2026, 8, 10, 10, 0));
    }

    #[test]
    fn next_monthly_clamps_to_month_end() {
        // 每月 31 号：2026-02-28（2 月无 31 号 → 28）。
        let from = dt(2026, 1, 15, 8, 0);
        assert_eq!(
            next_occurrence(&RepeatRule::Monthly(31), from),
            dt(2026, 1, 31, 8, 0)
        );
        let from2 = dt(2026, 2, 1, 8, 0);
        assert_eq!(
            next_occurrence(&RepeatRule::Monthly(31), from2),
            dt(2026, 2, 28, 8, 0)
        );
        // 闰年 2 月 → 29。
        let from3 = dt(2024, 2, 1, 8, 0);
        assert_eq!(
            next_occurrence(&RepeatRule::Monthly(31), from3),
            dt(2024, 2, 29, 8, 0)
        );
    }

    #[test]
    fn next_yearly() {
        // 每年 12-25：今年已过 → 明年。
        let from = dt(2026, 12, 26, 9, 0);
        assert_eq!(
            next_occurrence(&RepeatRule::Yearly(12, 25), from),
            dt(2027, 12, 25, 9, 0)
        );
        // 今年未到 → 今年。
        let from2 = dt(2026, 1, 1, 9, 0);
        assert_eq!(
            next_occurrence(&RepeatRule::Yearly(12, 25), from2),
            dt(2026, 12, 25, 9, 0)
        );
        // 2-29 在非闰年 → 3-01（clamp 到 2 月最后一天 28）。
        let from3 = dt(2027, 1, 1, 0, 0);
        assert_eq!(
            next_occurrence(&RepeatRule::Yearly(2, 29), from3),
            dt(2027, 2, 28, 0, 0)
        );
    }

    // ---- describe ----

    #[test]
    fn describe_rules() {
        assert_eq!(describe(&RepeatRule::Daily), "每天");
        assert_eq!(describe(&RepeatRule::Weekly(vec![1, 3, 5])), "每周周一、周三、周五");
        assert_eq!(describe(&RepeatRule::IntervalDays(2)), "每 2 天");
        assert_eq!(describe(&RepeatRule::Monthly(15)), "每月 15 号");
        assert_eq!(describe(&RepeatRule::Yearly(12, 25)), "每年 12 月 25 日");
    }
}
