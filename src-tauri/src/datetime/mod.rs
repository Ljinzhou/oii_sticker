//! 时间工具模块：独立的时间日期逻辑，供截止日期、提醒设置、调度器复用。
//!
//! 刻意不引入 chrono（保持轻量），日期算法自实现：
//! - 儒略式天数累计，1970-01-01 为 epoch day 0（星期四）；
//! - 星期 = (epoch_days + 4) mod 7（0 = 周日）。
//!
//! 子模块：
//! - [`parse`]：字符串解析 / 格式化（"YYYY-MM-DD HH:MM"）；
//! - [`repeat`]：重复提醒规则（周几 / 每 N 天 / 每月 / 每年）。

pub mod parse;
pub mod repeat;

/// 日期时间（本地时间语义，无时区）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DateTime {
    pub year: u32,
    /// 1-12
    pub month: u32,
    /// 1-31
    pub day: u32,
    /// 0-23
    pub hour: u32,
    /// 0-59
    pub minute: u32,
}

/// 各月天数（非闰年）。
const DAYS_IN_MONTH: [u32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

pub fn is_leap(year: u32) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

/// 某年某月的天数。
pub fn days_in_month(year: u32, month: u32) -> u32 {
    if !(1..=12).contains(&month) {
        return 0;
    }
    let base = DAYS_IN_MONTH[(month - 1) as usize];
    if month == 2 && is_leap(year) {
        base + 1
    } else {
        base
    }
}

impl DateTime {
    /// 构造（校验字段范围）；非法返回 None。
    pub fn new(year: u32, month: u32, day: u32, hour: u32, minute: u32) -> Option<Self> {
        let dt = Self {
            year,
            month,
            day,
            hour,
            minute,
        };
        dt.is_valid().then_some(dt)
    }

    /// 仅日期（时分归零）。
    pub fn from_ymd(year: u32, month: u32, day: u32) -> Option<Self> {
        Self::new(year, month, day, 0, 0)
    }

    pub fn is_valid(&self) -> bool {
        (1..=12).contains(&self.month)
            && self.day >= 1
            && self.day <= days_in_month(self.year, self.month)
            && self.hour <= 23
            && self.minute <= 59
    }

    /// 当前本地时间。
    ///
    /// 源项目在 Windows 上直调 `windows-sys GetLocalTime`（本工程红线禁用
    /// windows-sys 直调），改用已在依赖树中的 `time` crate 获取本地时区偏移
    /// （`UtcOffset::current_local_offset` 内部跨平台实现）。
    pub fn now() -> Self {
        let offset = time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC);
        let now = time::OffsetDateTime::now_utc().to_offset(offset);
        Self::new(
            now.year() as u32,
            u8::from(now.month()) as u32,
            now.day() as u32,
            now.hour() as u32,
            now.minute() as u32,
        )
        .unwrap_or(Self {
            year: 1970,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
        })
    }

    /// 自 1970-01-01 起的天数（儒略式累计）。
    pub fn epoch_days(&self) -> i64 {
        let mut days: i64 = 0;
        for year in 1970..self.year {
            days += if is_leap(year) { 366 } else { 365 };
        }
        for month in 1..self.month {
            days += days_in_month(self.year, month) as i64;
        }
        days += (self.day - 1) as i64;
        days
    }

    /// 转 epoch 秒（1970-01-01 00:00 UTC 语义；本地时间直接换算）。
    pub fn to_epoch_secs(self) -> Option<u64> {
        if !self.is_valid() {
            return None;
        }
        let secs = self.epoch_days() * 86400
            + self.hour as i64 * 3600
            + self.minute as i64 * 60;
        u64::try_from(secs).ok()
    }

    /// 从 epoch 秒还原。
    pub fn from_epoch_secs(secs: u64) -> Option<Self> {
        let days = (secs / 86400) as i64;
        let rem = secs % 86400;
        let hour = (rem / 3600) as u32;
        let minute = ((rem % 3600) / 60) as u32;

        // 天数 → (year, month, day)
        let mut year = 1970i64;
        let mut remaining = days;
        loop {
            let ylen = if is_leap(year as u32) { 366 } else { 365 };
            if remaining < ylen {
                break;
            }
            remaining -= ylen;
            year += 1;
        }
        let mut month = 1u32;
        loop {
            let mlen = days_in_month(year as u32, month) as i64;
            if remaining < mlen {
                break;
            }
            remaining -= mlen;
            month += 1;
        }
        Self::new(year as u32, month, (remaining + 1) as u32, hour, minute)
    }

    /// 星期几：0 = 周日，1 = 周一，……，6 = 周六。
    pub fn weekday(&self) -> u32 {
        // 1970-01-01 是星期四（epoch day 0 → 4）。
        (self.epoch_days() + 4).rem_euclid(7) as u32
    }

    /// 日期部分（时分归零）。
    pub fn date_only(&self) -> Self {
        Self {
            year: self.year,
            month: self.month,
            day: self.day,
            hour: 0,
            minute: 0,
        }
    }

    /// 加上若干天（允许负值），时间保持不变。
    pub fn add_days(&self, days: i64) -> Self {
        let secs = self
            .to_epoch_secs()
            .unwrap_or(0) as i64
            + days * 86400;
        Self::from_epoch_secs(secs.max(0) as u64).unwrap_or(*self)
    }

    /// 加上若干分钟（允许负值）。
    pub fn add_minutes(&self, minutes: i64) -> Self {
        let secs = self.to_epoch_secs().unwrap_or(0) as i64 + minutes * 60;
        Self::from_epoch_secs(secs.max(0) as u64).unwrap_or(*self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_validates_range() {
        assert!(DateTime::new(2026, 2, 28, 12, 30).is_some());
        assert!(DateTime::new(2026, 2, 29, 12, 30).is_none(), "2026 非闰年");
        assert!(DateTime::new(2024, 2, 29, 12, 30).is_some(), "2024 闰年");
        assert!(DateTime::new(2026, 13, 1, 0, 0).is_none());
        assert!(DateTime::new(2026, 1, 32, 0, 0).is_none());
        assert!(DateTime::new(2026, 1, 1, 24, 0).is_none());
    }

    #[test]
    fn epoch_roundtrip() {
        for dt in [
            DateTime::new(1970, 1, 1, 0, 0).unwrap(),
            DateTime::new(2026, 8, 1, 23, 59).unwrap(),
            DateTime::new(2000, 2, 29, 12, 0).unwrap(), // 闰日
        ] {
            let secs = dt.to_epoch_secs().unwrap();
            assert_eq!(DateTime::from_epoch_secs(secs).unwrap(), dt);
        }
    }

    #[test]
    fn epoch_days_known_values() {
        // 1970-01-01 = 0，1970-01-02 = 1。
        assert_eq!(DateTime::from_ymd(1970, 1, 1).unwrap().epoch_days(), 0);
        assert_eq!(DateTime::from_ymd(1970, 1, 2).unwrap().epoch_days(), 1);
        // 1970-12-31 = 364。
        assert_eq!(DateTime::from_ymd(1970, 12, 31).unwrap().epoch_days(), 364);
        // 1971-01-01 = 365。
        assert_eq!(DateTime::from_ymd(1971, 1, 1).unwrap().epoch_days(), 365);
    }

    #[test]
    fn weekday_known_values() {
        // 1970-01-01 星期四。
        assert_eq!(DateTime::from_ymd(1970, 1, 1).unwrap().weekday(), 4);
        // 1970-01-04 星期日。
        assert_eq!(DateTime::from_ymd(1970, 1, 4).unwrap().weekday(), 0);
        // 2026-08-01 星期六。
        assert_eq!(DateTime::from_ymd(2026, 8, 1).unwrap().weekday(), 6);
        // 2026-08-03 星期一。
        assert_eq!(DateTime::from_ymd(2026, 8, 3).unwrap().weekday(), 1);
    }

    #[test]
    fn add_days_crosses_month_and_year() {
        let d = DateTime::from_ymd(2026, 1, 31).unwrap();
        assert_eq!(d.add_days(1), DateTime::from_ymd(2026, 2, 1).unwrap());
        assert_eq!(d.add_days(-1), DateTime::from_ymd(2026, 1, 30).unwrap());
        let y = DateTime::from_ymd(2026, 12, 31).unwrap();
        assert_eq!(y.add_days(1), DateTime::from_ymd(2027, 1, 1).unwrap());
    }

    #[test]
    fn days_in_month_leap() {
        assert_eq!(days_in_month(2026, 2), 28);
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2026, 12), 31);
    }

    #[test]
    fn now_is_valid() {
        let n = DateTime::now();
        assert!(n.is_valid());
        // 与 epoch 秒往返一致
        assert_eq!(DateTime::from_epoch_secs(n.to_epoch_secs().unwrap()).unwrap(), n);
    }
}
