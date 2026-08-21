//! 后台定时任务：每隔 N 秒扫一次 DB，对到达提醒时间的 sticker 触发
//! `alert_active` 状态事件 + 系统通知。
//!
//! 设计要点（语义平移自旧项目 `reminder/scheduler.rs`）：
//! - 用 Tauri runtime 的 `time::interval` 走 10s 周期，独立于 UI 线程；
//! - 命中后发事件 + `tauri-plugin-notification` 通知（无动画）；
//! - 一次性提醒：触发后清空 `remind_at`；循环提醒按 `remind_rule` 重新计算；
//! - 下一次以**原定 remind_at** 为基准推进（避免漂移），关机长时间后
//!   从原定时间循环推进到未来（上限 366 次）。

use std::time::Duration;

use anyhow::Result;
use tauri::AppHandle;
use tokio::time::{interval, MissedTickBehavior};
use tracing::{info, warn};

use crate::commands;
use crate::datetime::{self, DateTime};
use crate::events;
use crate::models::StickerAttrs;
use crate::state::AppState;

/// 启动调度器（在 `setup` 内调用一次；返回 JoinHandle）。
pub fn spawn(app: AppHandle, state: AppState) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        let mut ticker = interval(Duration::from_secs(10));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        info!("[提醒] 调度器已启动，周期 10s");
        loop {
            ticker.tick().await;
            if let Err(e) = tick_once(&app, &state).await {
                warn!("[提醒] 本轮扫描失败：{e:#}");
            }
        }
    })
}

async fn tick_once(app: &AppHandle, state: &AppState) -> Result<()> {
    let due = state
        .with_conn_async(collect_due_sync)
        .await?;
    if !due.is_empty() {
        info!("命中 {} 个待提醒便签", due.len());
    }
    for attrs in due {
        // 1) alert_active 状态信号（无动画，前端自行表现）
        events::emit_alert_active(app, attrs.sticker_id, true);

        // 2) 系统通知
        let db = state.db_path();
        let sticker = state
            .with_conn(|c| commands::get_sticker(c, attrs.sticker_id, &db))
            .ok()
            .flatten();
        let title = sticker
            .as_ref()
            .map(|s| s.title.clone())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| format!("便签 #{}", attrs.sticker_id));
        let body = attrs
            .due_date
            .clone()
            .or_else(|| attrs.remind_at.clone())
            .unwrap_or_else(|| "提醒时间到".to_string());
        crate::platform::notify::send(app, &title, &body);

        // 3) 更新 sticker_attrs：一次性清空，循环性按规则计算下一次。
        //    下一次以「原定 remind_at」为基准推进（避免漂移），关机
        //    长时间后从原定时间循环推进到未来（见 compute_next_remind_at）。
        let next = if attrs.is_recurring {
            compute_next_remind_at(&attrs, DateTime::now())
        } else {
            None
        };

        let updated = StickerAttrs {
            sticker_id: attrs.sticker_id,
            due_date: attrs.due_date,
            remind_at: next,
            remind_rule: attrs.remind_rule,
            is_recurring: attrs.is_recurring,
        };

        // 直接 await，避免在 async 任务内 block_on（P0-2）。
        if let Err(e) = state
            .with_conn_async(move |conn| commands::set_reminder(conn, &updated))
            .await
        {
            warn!("[提醒] 便签 #{} 更新提醒失败：{e:#}", attrs.sticker_id);
        }
    }
    Ok(())
}

/// 旧格式提醒规则（`30m` / `1h` / `2d` / `daily` / `weekly`）的相对偏移，
/// 按**本地时间**语义计算（与 datetime 模块一致，避免 UTC 换算时区偏移）。
fn legacy_offset_rule(rule: &str, now: DateTime) -> Option<DateTime> {
    let lower = rule.trim().to_lowercase();
    let minutes = if let Some(n) = lower.strip_suffix('m') {
        n.parse::<i64>().ok()?.saturating_mul(1)
    } else if let Some(n) = lower.strip_suffix('h') {
        n.parse::<i64>().ok()?.saturating_mul(60)
    } else if let Some(n) = lower.strip_suffix('d') {
        n.parse::<i64>().ok()?.saturating_mul(24 * 60)
    } else if lower == "daily" {
        24 * 60
    } else if lower == "weekly" {
        7 * 24 * 60
    } else {
        return None;
    };
    Some(now.add_minutes(minutes))
}

/// 计算循环提醒的下一次触发时间（ISO 字符串）。
///
/// - 从**原定 remind_at**（而非触发时刻 now）为基准推进，避免漂移；
/// - 关机期间错过的周期：循环推进直到严格晚于 `now`（最大 366 次，
///   防止异常规则导致死循环）。
fn compute_next_remind_at(attrs: &StickerAttrs, now: DateTime) -> Option<String> {
    let base = attrs
        .remind_at
        .as_deref()
        .and_then(datetime::parse::parse)?;
    let mut next = advance_from(attrs, base)?;
    let mut guard = 0;
    // 上限 366 次：防异常规则死循环。极端情况下（如关机超一年）
    // 触达上限后 next 仍可能 ≤ now，下轮 tick 会再次命中并继续追补——
    // 每 10s 触发一次直至追上，不会死循环。
    while next <= now && guard < 366 {
        next = advance_from(attrs, next)?;
        guard += 1;
    }
    Some(format_iso_ts(&next))
}

/// 从 `from` 推进一次（新规则或旧格式规则）。
fn advance_from(attrs: &StickerAttrs, from: DateTime) -> Option<DateTime> {
    attrs
        .remind_rule
        .as_deref()
        .and_then(datetime::repeat::parse_rule)
        .map(|r| datetime::repeat::next_occurrence(&r, from))
        .or_else(|| legacy_offset_rule(attrs.remind_rule.as_deref()?, from))
}

/// 同步读取所有到达提醒时间的 sticker_attrs（不能在 async 里直接 lock Mutex）。
fn collect_due_sync(conn: &rusqlite::Connection) -> Result<Vec<StickerAttrs>> {
    // 用本地时间语义比较：remind_at 是"本地时间"字符串，
    // 与 DateTime::now()（本地时间）直接比较，避免 UTC 换算的时区偏移。
    let now = DateTime::now();
    let mut stmt = conn.prepare_cached(
        "SELECT sticker_id, due_date, remind_at, remind_rule, is_recurring
           FROM sticker_attrs
          WHERE remind_at IS NOT NULL",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(StickerAttrs {
                sticker_id: row.get(0)?,
                due_date: row.get(1)?,
                remind_at: row.get(2)?,
                remind_rule: row.get(3)?,
                is_recurring: row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows
        .into_iter()
        .filter(|a| {
            a.remind_at
                .as_deref()
                .and_then(datetime::parse::parse)
                .map(|dt| dt <= now)
                .unwrap_or(false)
        })
        .collect())
}

/// 把 `DateTime` 格式化为 SQLite 可比较的 ISO 字符串。
fn format_iso_ts(dt: &DateTime) -> String {
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:00",
        dt.year, dt.month, dt.day, dt.hour, dt.minute
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(y: u32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime {
        DateTime::new(y, mo, d, h, mi).unwrap()
    }

    /// 旧格式规则按本地时间语义换算（分钟级偏移）。
    #[test]
    fn legacy_offset_local_semantics() {
        let now = dt(2026, 8, 1, 10, 0);
        assert_eq!(legacy_offset_rule("30m", now), Some(dt(2026, 8, 1, 10, 30)));
        assert_eq!(legacy_offset_rule("1h", now), Some(dt(2026, 8, 1, 11, 0)));
        assert_eq!(legacy_offset_rule("2d", now), Some(dt(2026, 8, 3, 10, 0)));
        assert_eq!(legacy_offset_rule("daily", now), Some(dt(2026, 8, 2, 10, 0)));
        assert_eq!(legacy_offset_rule("weekly", now), Some(dt(2026, 8, 8, 10, 0)));
    }

    /// 非法 / 空规则返回 None。
    #[test]
    fn legacy_offset_invalid() {
        let now = dt(2026, 8, 1, 10, 0);
        assert!(legacy_offset_rule("", now).is_none());
        assert!(legacy_offset_rule("foo", now).is_none());
        assert!(legacy_offset_rule("30x", now).is_none());
        // 大小写不敏感。
        assert_eq!(legacy_offset_rule("DAILY", now), Some(dt(2026, 8, 2, 10, 0)));
    }

    /// 回归（P1-7）：下一次触发以**原定 remind_at** 为基准，
    /// 不能以触发时刻 now 为基准（否则每次触发时间都会漂移）。
    #[test]
    fn recurring_next_uses_original_time_not_now() {
        let attrs = StickerAttrs {
            sticker_id: 1,
            due_date: None,
            remind_at: Some("2026-08-01T10:00:00".to_string()),
            remind_rule: Some("daily".to_string()),
            is_recurring: true,
        };
        // 原定 10:00，实际 10:05 才触发：daily 下一次必须是次日 10:00。
        let now = dt(2026, 8, 1, 10, 5);
        assert_eq!(
            compute_next_remind_at(&attrs, now).as_deref(),
            Some("2026-08-02T10:00:00")
        );
    }

    /// 长时间关机后原定时间已过去：从原定时间循环推进到未来第一个时点。
    #[test]
    fn recurring_catch_up_skips_past_occurrences() {
        let attrs = StickerAttrs {
            sticker_id: 1,
            due_date: None,
            remind_at: Some("2026-08-01T10:00:00".to_string()),
            remind_rule: Some("daily".to_string()),
            is_recurring: true,
        };
        // 关机 3 天，8-04 09:00 开机：下一次应为 8-04 10:00（跳过 8/2、8/3）。
        let now = dt(2026, 8, 4, 9, 0);
        assert_eq!(
            compute_next_remind_at(&attrs, now).as_deref(),
            Some("2026-08-04T10:00:00")
        );
    }

    /// 旧格式相对偏移规则同样以原定时间为基准。
    #[test]
    fn recurring_legacy_rule_keeps_base_time() {
        let attrs = StickerAttrs {
            sticker_id: 1,
            due_date: None,
            remind_at: Some("2026-08-01T10:00:00".to_string()),
            remind_rule: Some("1h".to_string()),
            is_recurring: true,
        };
        let now = dt(2026, 8, 1, 10, 0);
        assert_eq!(
            compute_next_remind_at(&attrs, now).as_deref(),
            Some("2026-08-01T11:00:00")
        );
    }

    /// 追补上限：极端规则（如 interval:0 解析失败返回 None）不 panic。
    #[test]
    fn compute_next_invalid_rule_returns_none() {
        let attrs = StickerAttrs {
            sticker_id: 1,
            due_date: None,
            remind_at: Some("2026-08-01T10:00:00".to_string()),
            remind_rule: Some("bogus".to_string()),
            is_recurring: true,
        };
        assert!(compute_next_remind_at(&attrs, dt(2026, 8, 1, 10, 0)).is_none());
    }
}
