//! Todo 提醒调度器：独立后台线程管理 todo_blocks 的 reminder_at / due_at 触发。
//!
//! 设计（替代 v12 前旧便签提醒系统，语义按新 TodoBlock 模型重写）：
//! - **独立线程**：`std::thread`（名字 `todo-reminder`）+ `Condvar` 定时等待，
//!   完全独立于任何窗口生命周期——便签/Todo 窗口全部关闭后仍照常触发，
//!   只要主进程存活（托盘常驻）。
//! - **动态睡眠**：每轮扫描后计算「下一个未触发时点」的距离，钳制在
//!   [1s, 30s]；期间任意命令改动提醒数据可经 [`ReminderSignal::wake`]
//!   立即唤醒重扫，无需等满周期。
//! - **触发回调**：命中到期任务后依次执行注册的 [`FireHook`] 回调链
//!   （系统通知 + 广播事件），新增提醒表现方式只需再注册钩子。
//! - **不重复触发**：`reminded_at` / `due_notified_at` 列记录已触发状态；
//!   循环任务（repeat_rule）按规则从原定时间推进到未来下一次（防漂移、
//!   关机追补上限 366 次）。
//!
//! 时间格式约定：前端经 `dayjs.toISOString()` 写入 UTC ISO（RFC3339，
//! 含毫秒与 Z）。解析兼容无毫秒 / `+00:00` 后缀 / SQLite 空格格式。

use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::Serialize;
use tauri::{AppHandle, Manager};
use tracing::{debug, info, warn};

use crate::state::AppState;

/// 无近期任务时的轮询间隔（也是单次睡眠上限，防系统时钟跳变睡过头）。
const MAX_SLEEP: Duration = Duration::from_secs(30);
/// 有未来任务时睡眠下限（到期即刻触发，不做亚秒级空转）。
const MIN_SLEEP_SECS: u64 = 1;
/// 循环规则追补上限：超过则放弃推进（防异常规则死循环）。
const CATCH_UP_CAP: u32 = 366;

const DAY_MS: i64 = 86_400_000;
const HOUR_MS: i64 = 3_600_000;

// ═══════════════════ 时间解析 / 格式化 ═══════════════════

/// 解析 UTC ISO 时间为 Unix 毫秒时间戳。
///
/// 兼容：`2026-08-25T07:00:00.000Z` / `2026-08-25T07:00:00Z` /
/// `2026-08-25T07:00:00+00:00` / `2026-08-25 07:00:00`（SQLite 格式，UTC 语义）。
pub fn parse_utc_ms(raw: &str) -> Option<i64> {
    use time::format_description::well_known::Rfc3339;
    use time::OffsetDateTime;
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    // SQLite datetime('now') 风格：空格分隔 → 视作 UTC 补全为 RFC3339。
    let normalized = if s.len() >= 19 && s.as_bytes().get(10) == Some(&b' ') {
        format!("{}T{}Z", &s[..10], &s[11..19])
    } else {
        s.to_string()
    };
    OffsetDateTime::parse(&normalized, &Rfc3339)
        .map(|dt| dt.unix_timestamp() * 1000 + i64::from(dt.millisecond()))
        .ok()
}

/// 把 Unix 毫秒格式化为前端 dayjs 可解析的 UTC ISO 字符串（毫秒精度）。
pub fn format_utc_ms(ms: i64) -> Option<String> {
    use time::format_description::well_known::Rfc3339;
    use time::OffsetDateTime;
    let frac = u16::try_from(ms.rem_euclid(1000)).ok()?;
    let dt = OffsetDateTime::from_unix_timestamp(ms.div_euclid(1000))
        .ok()?
        .replace_millisecond(frac)
        .ok()?;
    dt.format(&Rfc3339).ok()
}

fn now_utc_ms() -> i64 {
    use time::OffsetDateTime;
    let dt = OffsetDateTime::now_utc();
    dt.unix_timestamp() * 1000 + i64::from(dt.millisecond())
}

// ═══════════════════ 到期扫描 ═══════════════════

/// 提醒类别：到点提醒 / 截止提醒。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReminderKind {
    /// reminder_at 到点
    Reminder,
    /// due_at 截止到点
    Due,
}

/// 一次触发的完整上下文（传给回调链 / 前端事件负载）。
#[derive(Debug, Clone, Serialize)]
pub struct FireContext {
    pub id: String,
    pub sticker_id: i64,
    pub title: String,
    pub block_title: String,
    pub kind: ReminderKind,
    /// 原定的 reminder_at / due_at 原始串（供展示）。
    pub scheduled_at: String,
}

impl FireContext {
    pub fn kind_label(&self) -> &'static str {
        match self.kind {
            ReminderKind::Reminder => "提醒",
            ReminderKind::Due => "截止",
        }
    }
}

/// 待判定任务行。
#[derive(Debug, Clone)]
struct PendingTodo {
    id: String,
    sticker_id: i64,
    title: String,
    block_title: String,
    reminder_at: Option<String>,
    due_at: Option<String>,
    repeat_rule: Option<String>,
    reminded_at: Option<String>,
    due_notified_at: Option<String>,
    reminder_ack_at: Option<String>,
    due_ack_at: Option<String>,
}

/// 单个字段（reminder/due）的触发资格判定结果。
enum Eligibility {
    /// 已到期待触发
    DueNow,
    /// 未来时点（参与睡眠计算）
    Future(i64),
    /// 无需处理（未设置 / 已触发待确认 / 已被确认豁免）
    Idle,
}

/// 判定某字段的触发资格。
///
/// 确认语义：`ack` 只豁免「不晚于确认时刻」的提醒——用户确认收到后，
/// 同一时点的提醒重启也不再弹；而循环任务推进出的后续周期
/// （晚于确认时刻）照常触发。已触发但未确认的行不再进入扫描
/// （标记列非空即视为提醒中）。
fn eligibility(
    scheduled: &Option<String>,
    fired_flag: &Option<String>,
    ack: &Option<String>,
    now: i64,
) -> Eligibility {
    let Some(ms) = scheduled.as_deref().and_then(parse_utc_ms) else {
        return Eligibility::Idle;
    };
    if fired_flag.is_some() {
        return Eligibility::Idle; // 已触发、等待用户处理（高亮中），不重复触发
    }
    if let Some(ack_ms) = ack.as_deref().and_then(parse_utc_ms) {
        if ms <= ack_ms {
            return Eligibility::Idle; // 用户已确认过该时点，永久静默
        }
    }
    if ms <= now {
        Eligibility::DueNow
    } else {
        Eligibility::Future(ms)
    }
}

/// 扫描待触发的根任务（子任务的日期由父块统一承载，不单独设提醒）。
///
/// 返回到期列表与「下一个未来未触发时点」（供睡眠时长计算）。
fn collect_pending(
    conn: &rusqlite::Connection,
) -> Result<(Vec<(PendingTodo, ReminderKind)>, Option<i64>)> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, sticker_id, title, block_title, reminder_at, due_at, repeat_rule,
                reminded_at, due_notified_at, reminder_ack_at, due_ack_at
         FROM todo_blocks
        WHERE is_completed = 0 AND parent_id IS NULL
          AND (reminder_at IS NOT NULL OR due_at IS NOT NULL)
          AND (reminded_at IS NULL OR due_notified_at IS NULL)",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(PendingTodo {
                id: row.get(0)?,
                sticker_id: row.get(1)?,
                title: row.get(2)?,
                block_title: row.get(3)?,
                reminder_at: row.get(4)?,
                due_at: row.get(5)?,
                repeat_rule: row.get(6)?,
                reminded_at: row.get(7)?,
                due_notified_at: row.get(8)?,
                reminder_ack_at: row.get(9)?,
                due_ack_at: row.get(10)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let now = now_utc_ms();
    let mut due = Vec::new();
    let mut next_future: Option<i64> = None;
    for p in &rows {
        for (kind, scheduled, flag, ack) in [
            (ReminderKind::Reminder, &p.reminder_at, &p.reminded_at, &p.reminder_ack_at),
            (ReminderKind::Due, &p.due_at, &p.due_notified_at, &p.due_ack_at),
        ] {
            match eligibility(scheduled, flag, ack, now) {
                Eligibility::DueNow => due.push((p.clone(), kind)),
                Eligibility::Future(ms) => {
                    next_future = Some(next_future.map_or(ms, |m: i64| m.min(ms)));
                }
                Eligibility::Idle => {}
            }
        }
    }
    Ok((due, next_future))
}

// ═══════════════════ 循环规则推进 ═══════════════════

/// 计算循环任务的下一次触发时间（Unix 毫秒）。
///
/// - 以**原定时间** `base_ms` 为基准逐步推进到严格晚于 `now_ms`（防漂移；
///   关机/长时间未运行后自动追补跳过过去的时点，上限 [`CATCH_UP_CAP`] 次）；
/// - 规则为前端 RepeatPicker 写入的 JSON：
///   `{"unit":"day|week|month|year","interval":n,"weekdays":[0..6]}`，
///   weekdays 为 JS 语义（0=周日…6=周六）；
/// - 解析失败返回 None（调用方退化为一次性处理）。
pub fn next_occurrence_ms(rule: &str, base_ms: i64, now_ms: i64) -> Option<i64> {
    #[derive(serde::Deserialize)]
    struct Rule {
        unit: String,
        #[serde(default)]
        interval: Option<serde_json::Value>,
        #[serde(default)]
        weekdays: Option<Vec<f64>>,
    }
    let parsed: Rule = serde_json::from_str(rule).ok()?;
    // interval 容错：数字或缺失（默认 1）；非法值视为 1。
    let interval = match parsed.interval {
        Some(serde_json::Value::Number(n)) => n.as_i64().unwrap_or(1),
        _ => 1,
    };
    if interval < 1 {
        return None;
    }
    let weekdays: Vec<u32> = parsed
        .weekdays
        .unwrap_or_default()
        .into_iter()
        .filter_map(|d| u32::try_from(d as i64).ok())
        .filter(|d| *d <= 6)
        .collect();

    let step_ms: i64 = match parsed.unit.as_str() {
        "day" => DAY_MS.checked_mul(interval)?,
        "week" if !weekdays.is_empty() => DAY_MS, // 逐天试探 + 星期/周序校验
        "week" => DAY_MS.checked_mul(7 * interval)?,
        "month" | "year" => DAY_MS,               // 占位步长（add_months 实际推进）
        _ => return None,
    };
    let unit = parsed.unit;

    let mut cur = base_ms;
    for _ in 0..CATCH_UP_CAP {
        cur = match unit.as_str() {
            "month" => add_months_ms(cur, interval)?,
            "year" => add_months_ms(cur, interval.saturating_mul(12))?,
            _ => cur.checked_add(step_ms)?,
        };
        if cur > now_ms {
            // 周 + weekdays：还需落在指定的星期组合上（按 interval 周期）。
            if unit == "week" && !weekdays.is_empty() {
                if !weekdays.contains(&js_weekday_ms(cur))
                    || !on_pattern_week(cur, base_ms, interval)
                {
                    continue;
                }
            }
            return Some(cur);
        }
    }
    // 追补耗尽仍未越过 now（如关机超一年）：放弃推进，交由下轮 tick 再判。
    None
}

/// Unix 毫秒 → JS 星期（0=周日…6=周六）。1970-01-01 是周四（JS 4）。
fn js_weekday_ms(ms: i64) -> u32 {
    (ms.div_euclid(DAY_MS) + 4).rem_euclid(7) as u32
}

/// 判断候选时刻是否落在以 base 为锚点、间隔 interval 周的模式周上。
///
/// 锚点语义：候选日与基准日的自然周序差对 interval 取模为 0 即命中
/// （部分首周计入第 0 周）。
fn on_pattern_week(candidate_ms: i64, base_ms: i64, interval: i64) -> bool {
    let base_day = base_ms.div_euclid(DAY_MS);
    let cand_day = candidate_ms.div_euclid(DAY_MS);
    ((cand_day - base_day).div_euclid(7)).rem_euclid(interval.max(1)) == 0
}

/// 加 n 个自然月（日超月末则钳制到月末），保持时刻的时分秒。
fn add_months_ms(ms: i64, n: i64) -> Option<i64> {
    use time::{Date, Month, OffsetDateTime, Time};
    let dt = OffsetDateTime::from_unix_timestamp(ms.div_euclid(1000)).ok()?;
    let total = i64::from(dt.year()) * 12 + i64::from(u8::from(dt.month())) - 1 + n;
    let year = i32::try_from(total.div_euclid(12)).ok()?;
    let month_idx = u8::try_from(total.rem_euclid(12) + 1).ok()?;
    let month = Month::try_from(month_idx).ok()?;
    let day = dt.day().min(month.length(year));
    let date = Date::from_calendar_date(year, month, day).ok()?;
    let time_of_day = Time::from_hms(dt.hour(), dt.minute(), dt.second()).ok()?;
    let next = OffsetDateTime::new_utc(date, time_of_day);
    Some(next.unix_timestamp() * 1000 + i64::from(dt.millisecond()))
}

// ═══════════════════ 触发回调 ═══════════════════

/// 触发回调钩子。命中到期任务后按注册顺序依次执行。
pub trait FireHook: Send + Sync {
    fn on_fire(&self, app: &AppHandle, ctx: &FireContext);
}

/// 回调 ①：系统通知（tauri-plugin-notification，Windows Toast 等）。
struct SystemNotificationHook;

impl FireHook for SystemNotificationHook {
    fn on_fire(&self, app: &AppHandle, ctx: &FireContext) {
        let title = format!(" 任务{}：{}", ctx.kind_label(), display_title(ctx));
        let body = match ctx.kind {
            ReminderKind::Reminder => format!("设定的提醒时间已到（{}）", short_time(&ctx.scheduled_at)),
            ReminderKind::Due => format!("任务已到截止时间（{}）", short_time(&ctx.scheduled_at)),
        };
        // 直接用 notify-rust 在新线程上等待用户点击系统 Toast → 打开对应 todo 编辑窗口。
        // tauri-plugin-notification 的 Windows 桌面实现不暴露点击回调，故改用 notify-rust，
        // 其 Windows Toast 通过激活回调把点击/关闭事件写入通道，wait_for_action 根据动作开窗。
        let mut notification = notify_rust::Notification::new();
        notification.summary(&title);
        notification.body(&body);
        notification.app_id(app.config().identifier.as_str());
        match notification.show() {
            Ok(handle) => {
                let app = app.clone();
                let id = ctx.id.clone();
                std::thread::spawn(move || {
                    handle.wait_for_action(|action| {
                        // Windows 上点击通知体走默认 Action（非 "__closed"）；关闭/超时是 "__closed"。
                        if !action.eq_ignore_ascii_case("__closed") {
                            // 窗口创建/聚焦需在主线程执行，避免跨线程触碰 Webview 状态。
                            let app_for_open = app.clone();
                            let id_for_open = id.clone();
                            let open_app = app_for_open.clone();
                            let open_id = id_for_open.clone();
                            let _ = app_for_open.run_on_main_thread(move || {
                                open_todo_for_notification(&open_app, &open_id);
                            });
                        }
                    });
                });
            }
            Err(e) => {
                warn!("[提醒] 系统通知发送失败 todo={}: {e}", ctx.id);
            }
        }
    }
}

/// 回调 ②：广播事件（所有窗口弹应用内提示并刷新任务数据）。
struct BroadcastEventHook;

impl FireHook for BroadcastEventHook {
    fn on_fire(&self, app: &AppHandle, ctx: &FireContext) {
        crate::events::emit_todo_reminder(app, ctx);
        // 复用既有刷新管线：相关便签/Todo 窗口收到 todo://updated 后重新拉取
        // 块数据（新状态含 reminded_at / due_notified_at），驱动高亮渲染。
        crate::events::emit_todo_updated(app, ctx.sticker_id, &ctx.id);
    }
}

/// 默认回调链（系统通知 → 事件广播）。
pub fn default_hooks() -> Vec<Arc<dyn FireHook>> {
    vec![Arc::new(SystemNotificationHook), Arc::new(BroadcastEventHook)]
}

/// 点击系统通知后打开对应 todo 块编辑窗口。等价于前端 `open_todo_window_cmd`，
/// 但这里运行在 `wait_for_action` 线程里，直接复用阻塞式 DB 读取 + 同步创建窗口的低层路径，
/// 避免跨线程借用 command 的 `State`。
fn open_todo_for_notification(app: &AppHandle, id: &str) {
    // 校验任务存在，避免开出空窗
    let state = app.state::<AppState>();
    let block = state
        .with_conn(|c| crate::commands::get_todo_block(c, id))
        .ok()
        .flatten();
    if block.is_none() {
        return;
    }
    let always_on_top = state
        .with_conn(crate::commands::get_config)
        .map(|cfg| cfg.get_or("default_todo_always_on_top", "1") == "1")
        .unwrap_or(true);
    let label = format!("todo-{id}");
    if let Some(win) = app.get_webview_window(&label) {
        let _ = win.set_always_on_top(always_on_top);
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
        return;
    }
    if let Ok(win) = crate::create_todo_win(app, id, always_on_top) {
        let _ = win.set_always_on_top(always_on_top);
        let _ = win.set_focus();
    }
}

fn display_title(ctx: &FireContext) -> String {
    let source = if ctx.title.trim().is_empty() { &ctx.block_title } else { &ctx.title };
    let t = source.trim();
    if t.is_empty() { "未命名任务".to_string() } else { t.chars().take(40).collect() }
}

fn short_time(iso: &str) -> String {
    // 2026-08-25T07:05:00.000Z → 2026-08-25 07:05
    let s = iso.trim();
    if s.len() >= 16 {
        format!("{} {}", &s[..10], &s[11..16])
    } else {
        s.to_string()
    }
}

// ═══════════════════ 调度器线程 ═══════════════════

/// 唤醒信号：命令层改动提醒数据后调用 `wake()` 让调度线程立即重扫。
#[derive(Default)]
pub struct ReminderSignal {
    flag: Mutex<bool>,
    cond: Condvar,
}

impl ReminderSignal {
    /// 唤醒调度线程（立即重扫，不等本轮睡眠结束）。
    pub fn wake(&self) {
        if let Ok(mut f) = self.flag.lock() {
            *f = true;
            self.cond.notify_all();
        }
    }

    /// 睡眠至多 `timeout`；若期间被 wake（或 wake 早于本次调用），立即返回重扫。
    pub(crate) fn wait(&self, timeout: Duration) {
        let mut flag = match self.flag.lock() {
            Ok(f) => f,
            Err(poisoned) => poisoned.into_inner(),
        };
        if *flag {
            *flag = false; // 排空先到的唤醒信号：马上重扫
            return;
        }
        let (mut guard, _) = match self.cond.wait_timeout(flag, timeout) {
            Ok(pair) => pair,
            Err(poisoned) => poisoned.into_inner(),
        };
        *guard = false;
    }
}

/// 启动调度线程（在 `setup` 内调用一次）。
///
/// 线程独立于所有 Webview 窗口：便签/Todo 窗口全部关闭后依然按时触发，
/// 只要主进程存活（托盘常驻 / 开机自启场景）。
pub fn spawn(app: AppHandle, state: AppState) -> JoinHandle<()> {
    info!(
        "[提醒] 启动 Todo 提醒调度独立线程（动态睡眠 {}–{}s，支持命令层即时唤醒）",
        MIN_SLEEP_SECS,
        MAX_SLEEP.as_secs()
    );
    std::thread::Builder::new()
        .name("todo-reminder".into())
        .spawn(move || run_loop(app, state))
        .expect("创建 todo-reminder 线程失败")
}

fn run_loop(app: AppHandle, state: AppState) {
    loop {
        let fired = match state.with_conn(|c| collect_pending(c)) {
            Ok((due, _)) => {
                for (pending, kind) in &due {
                    if let Err(e) = fire_one(&app, &state, pending, *kind) {
                        warn!("[提醒] 触发处理失败 todo={}：{e:#}", pending.id);
                    }
                }
                due.len()
            }
            Err(e) => {
                warn!("[提醒] 扫描到期任务失败：{e:#}");
                0
            }
        };
        if fired > 0 {
            debug!("[提醒] 本轮触发 {fired} 项");
        }

        // 计算下一睡眠时长：到最近的未来时点的距离，钳制 [1s, 30s]。
        // （触发会改写标记列，因此用触发后的最新状态计算。）
        let sleep = state
            .with_conn(|c| collect_pending(c))
            .map(|(_, next)| {
                next.map_or(MAX_SLEEP, |next_ms| {
                    let secs = (next_ms - now_utc_ms()).max(0) as u64 / 1000;
                    Duration::from_secs(secs.clamp(MIN_SLEEP_SECS, MAX_SLEEP.as_secs()))
                })
            })
            .unwrap_or(MAX_SLEEP);
        state.reminder_wait(sleep);
    }
}

/// 触发单个到期任务：执行回调链 → 标记已触发（循环任务推进下次时间）。
fn fire_one(
    app: &AppHandle,
    state: &AppState,
    pending: &PendingTodo,
    kind: ReminderKind,
) -> Result<()> {
    let raw = match kind {
        ReminderKind::Reminder => pending.reminder_at.as_deref(),
        ReminderKind::Due => pending.due_at.as_deref(),
    };
    let Some(raw) = raw else { return Ok(()); };

    let ctx = FireContext {
        id: pending.id.clone(),
        sticker_id: pending.sticker_id,
        title: pending.title.clone(),
        block_title: pending.block_title.clone(),
        kind,
        scheduled_at: raw.to_string(),
    };
    info!(
        "[提醒] 触发 todo={} sticker={} kind={:?} scheduled={raw}",
        ctx.id, ctx.sticker_id, ctx.kind
    );

    // 1) 回调链：系统通知 + 事件广播（均不持有数据库锁）
    for hook in default_hooks() {
        hook.on_fire(app, &ctx);
    }

    // 2) 标记已触发（高亮持续到用户完成/重设任务）
    let now_iso = format_utc_ms(now_utc_ms()).context("格式化触发时间失败")?;
    let id = pending.id.clone();
    let flag_col = match kind {
        ReminderKind::Reminder => "reminded_at",
        ReminderKind::Due => "due_notified_at",
    };
    state.with_conn(move |conn| {
        conn.execute(
            &format!(
                "UPDATE todo_blocks SET {flag_col} = ?2, updated_at = datetime('now') WHERE id = ?1"
            ),
            rusqlite::params![id, now_iso],
        )
        .context("写入触发标记失败")?;
        Ok(())
    })?;

    // 3) 循环任务：把触发的字段推进到未来下一次（其余字段不动）
    let next_val = (|| {
        let rule = pending.repeat_rule.as_deref()?;
        let (field, col) = match kind {
            ReminderKind::Reminder => (&pending.reminder_at, "reminder_at"),
            ReminderKind::Due => (&pending.due_at, "due_at"),
        };
        let base = field.as_deref().and_then(parse_utc_ms)?;
        let next = next_occurrence_ms(rule, base, now_utc_ms())?;
        Some((col, format_utc_ms(next)?))
    })();
    if let Some((col, next_iso)) = next_val {
        debug!("[提醒] 循环任务 {} 推进 {col} → {next_iso}", ctx.id);
        let id = pending.id.clone();
        state.with_conn(move |conn| {
            conn.execute(
                &format!("UPDATE todo_blocks SET {col} = ?2 WHERE id = ?1"),
                rusqlite::params![id, next_iso],
            )
            .context("推进循环任务时间失败")?;
            Ok(())
        })?;
    }
    Ok(())
}

// ═══════════════════ 测试 ═══════════════════

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: i64 = HOUR_MS;
    const DAY: i64 = DAY_MS;

    /// 固定“当前时刻”：2026-08-25T08:00:00Z（周二）。
    const NOW: i64 = 1_787_644_800_000;

    #[test]
    fn now_constant_is_expected_instant() {
        // 与 parse 互证，防止手算错误悄悄改变其它用例的基准。
        assert_eq!(parse_utc_ms("2026-08-25T08:00:00.000Z"), Some(NOW));
    }

    #[test]
    fn parse_accepts_common_iso_shapes() {
        assert_eq!(parse_utc_ms("2026-08-25T08:00:00.000Z"), Some(NOW));
        assert_eq!(parse_utc_ms("2026-08-25T08:00:00Z"), Some(NOW));
        assert_eq!(parse_utc_ms("2026-08-25T08:00:00+00:00"), Some(NOW));
        // 带时区偏移：08:00+08:00 == 00:00Z
        assert_eq!(parse_utc_ms("2026-08-25T08:00:00+08:00"), Some(NOW - 8 * HOUR));
        // SQLite 空格格式（UTC 语义）
        assert_eq!(parse_utc_ms("2026-08-25 08:00:00"), Some(NOW));
        // 非法输入
        assert_eq!(parse_utc_ms(""), None);
        assert_eq!(parse_utc_ms("not-a-time"), None);
        assert_eq!(parse_utc_ms("2026-13-40T99:00:00Z"), None);
    }

    #[test]
    fn format_roundtrips_parse() {
        for ms in [NOW, NOW + 3_661_000, 0, 1] {
            let iso = format_utc_ms(ms).unwrap();
            assert_eq!(parse_utc_ms(&iso), Some(ms), "roundtrip 失败：{iso}");
        }
    }

    #[test]
    fn daily_repeat_advances_from_base_without_drift_and_catches_up() {
        let rule = r#"{"unit":"day","interval":1}"#;
        let base = NOW - HOUR; // 原定 07:00，现在 08:00
        // 下一次应为次日 07:00（基于原定时间，而非触发时刻——防漂移）
        assert_eq!(next_occurrence_ms(rule, base, NOW), Some(NOW + 23 * HOUR));

        // 关机 3 天后开机：自动追补，直接落到未来的第一个 07:00
        let late_now = NOW + 3 * DAY;
        assert_eq!(
            next_occurrence_ms(rule, base, late_now),
            Some(late_now + 23 * HOUR)
        );
    }

    #[test]
    fn interval_repeat_multiplies_step() {
        let rule = r#"{"unit":"day","interval":2}"#;
        let base = NOW - HOUR;
        assert_eq!(next_occurrence_ms(rule, base, NOW), Some(NOW + 47 * HOUR));
    }

    #[test]
    fn weekly_with_weekdays_picks_next_matching_day() {
        // 今天是周二。指定每周一、三（weekdays 为 JS 语义 [1,3]）→ 下一个是周三。
        let rule = r#"{"unit":"week","interval":1,"weekdays":[1,3]}"#;
        let got = next_occurrence_ms(rule, NOW, NOW).unwrap();
        assert_eq!(js_weekday_ms(got), 3, "应落在周三");
        assert_eq!(got - NOW, DAY, "周二的下一天就是周三，时刻保持 08:00");

        // 只指定周五（[5]）→ 下一个周五是 3 天后
        let rule_fri = r#"{"unit":"week","interval":1,"weekdays":[5]}"#;
        let got = next_occurrence_ms(rule_fri, NOW, NOW).unwrap();
        assert_eq!(got - NOW, 3 * DAY);
    }

    #[test]
    fn biweekly_weekday_counts_partial_first_week() {
        // 每两周周一（[1]），锚点为本周二：即将到来的周一属于第 0 模式周 → 6 天后。
        let rule = r#"{"unit":"week","interval":2,"weekdays":[1]}"#;
        let got = next_occurrence_ms(rule, NOW, NOW).unwrap();
        assert_eq!(got - NOW, 6 * DAY);
        assert_eq!(js_weekday_ms(got), 1);

        // 从该周一再起算：下一个模式周一是 14 天后。
        let next2 = next_occurrence_ms(rule, got, got).unwrap();
        assert_eq!(next2 - got, 14 * DAY);
    }

    #[test]
    fn monthly_repeat_clamps_to_month_end() {
        // 1 月 31 日 → 2 月没有 31 日，应钳制到 2 月 28 日（2027 非闰年）。
        let jan31 = parse_utc_ms("2027-01-31T09:00:00Z").unwrap();
        let now = parse_utc_ms("2027-02-01T00:00:00Z").unwrap();
        let got = next_occurrence_ms(r#"{"unit":"month","interval":1}"#, jan31, now).unwrap();
        assert_eq!(got, parse_utc_ms("2027-02-28T09:00:00Z").unwrap());
    }

    #[test]
    fn yearly_repeat_adds_twelve_months() {
        let base = parse_utc_ms("2026-08-25T08:00:00Z").unwrap();
        let now = parse_utc_ms("2026-08-25T08:00:01Z").unwrap();
        let got = next_occurrence_ms(r#"{"unit":"year","interval":1}"#, base, now).unwrap();
        assert_eq!(got, parse_utc_ms("2027-08-25T08:00:00Z").unwrap());
    }

    #[test]
    fn invalid_rules_return_none() {
        assert_eq!(next_occurrence_ms("not json", NOW, NOW), None);
        assert_eq!(next_occurrence_ms(r#"{"unit":"decade"}"#, NOW, NOW), None);
        assert_eq!(
            next_occurrence_ms(r#"{"unit":"day","interval":0}"#, NOW, NOW),
            None
        );
    }

    #[test]
    fn catch_up_converges_for_long_gone_base() {
        // 基准在 300 天前：有限步内收敛到未来时点（CAP 保护不失控）。
        let base = NOW - 300 * DAY;
        let got = next_occurrence_ms(r#"{"unit":"day","interval":1}"#, base, NOW);
        assert!(got.is_some_and(|ms| ms > NOW));
    }

    #[test]
    fn js_weekday_matches_known_dates() {
        assert_eq!(js_weekday_ms(NOW), 2, "2026-08-25 应为周二");
        assert_eq!(js_weekday_ms(0), 4, "1970-01-01 应为周四");
    }

    #[test]
    fn short_time_formats_iso_for_display() {
        assert_eq!(short_time("2026-08-25T07:05:00.000Z"), "2026-08-25 07:05");
    }

    #[test]
    fn signal_wake_drains_before_wait() {
        let signal = ReminderSignal::default();
        signal.wake();
        // 先 wake 后 wait：应立即返回（排空标记），不阻塞。
        let start = std::time::Instant::now();
        signal.wait(Duration::from_secs(10));
        assert!(start.elapsed() < Duration::from_millis(500), "先 wake 后 wait 不应阻塞");

        // 再次 wait：无新信号则等满超时。
        let start = std::time::Instant::now();
        signal.wait(Duration::from_millis(120));
        assert!(start.elapsed() >= Duration::from_millis(100), "无信号时应等待超时");
    }

    // ═══════════ 确认（红点）语义 ═══════════

    use crate::db::{schema as db_schema, sticker_repo, todo_block_repo};

    /// 相对当前时刻 ±小时 的 UTC ISO 串。
    fn iso_from_now(hours: i64) -> String {
        format_utc_ms(now_utc_ms() + hours * HOUR_MS).unwrap()
    }

    fn ack_test_conn() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON").unwrap();
        db_schema::run_migrations(&conn).unwrap();
        sticker_repo::insert(
            &conn,
            &sticker_repo::NewSticker { title: "提醒测试".into(), ..Default::default() },
        )
        .unwrap();
        conn
    }

    fn make_block(conn: &rusqlite::Connection, reminder_hours: i64) -> String {
        let sid: i64 = conn
            .query_row("SELECT id FROM stickers LIMIT 1", [], |r| r.get(0))
            .unwrap();
        let block = todo_block_repo::create(conn, sid, None).unwrap();
        conn.execute(
            "UPDATE todo_blocks SET reminder_at = ?2 WHERE id = ?1",
            rusqlite::params![block.id, iso_from_now(reminder_hours)],
        )
        .unwrap();
        block.id
    }

    fn pending_ids(conn: &rusqlite::Connection) -> Vec<String> {
        collect_pending(conn)
            .unwrap()
            .0
            .into_iter()
            .map(|(p, _)| p.id)
            .collect()
    }

    /// 核心场景（需求 2）：13:00 提醒 → 用户点击红点确认 → 13:10 重启程序，
    /// 同一提醒不得再次触发。
    #[test]
    fn acked_reminder_stays_silent_after_restart() {
        let conn = ack_test_conn();
        let id = make_block(&conn, -1); // 1 小时前的提醒，已到期
        assert_eq!(pending_ids(&conn), [id.clone()], "未确认前应到期触发");

        // 调度器触发（写 reminded_at）→ 用户点红点确认（写 ack、清标记）
        conn.execute(
            "UPDATE todo_blocks SET reminded_at = ?2 WHERE id = ?1",
            rusqlite::params![id, iso_from_now(0)],
        )
        .unwrap();
        todo_block_repo::ack_alerts(&conn, &id).unwrap().unwrap();

        // 重启后再扫 N 轮：确认过的提醒必须保持静默
        for _ in 0..3 {
            assert!(pending_ids(&conn).is_empty(), "确认后不得再触发");
        }
    }

    /// 确认只豁免「不晚于确认时刻」的提醒：循环任务推进出的后续周期照常触发。
    #[test]
    fn ack_does_not_block_later_cycles() {
        let conn = ack_test_conn();
        let id = make_block(&conn, -5); // 更早到期的提醒
        // 模拟更早时刻的确认（早于本次提醒时间）：后续周期不受豁免
        conn.execute(
            "UPDATE todo_blocks SET reminder_ack_at = ?2 WHERE id = ?1",
            rusqlite::params![id, iso_from_now(-6)],
        )
        .unwrap();
        assert_eq!(pending_ids(&conn), [id.clone()], "晚于确认时刻的周期应正常触发");

        // 反向：确认时刻晚于提醒时间 → 静默
        conn.execute(
            "UPDATE todo_blocks SET reminder_ack_at = ?2 WHERE id = ?1",
            rusqlite::params![id, iso_from_now(0)],
        )
        .unwrap();
        assert!(pending_ids(&conn).is_empty(), "早于确认时刻的提醒应被豁免");
    }

    /// 未触发过的未来提醒不受历史确认影响；due 与 reminder 相互独立。
    #[test]
    fn future_and_due_kinds_are_independent() {
        let conn = ack_test_conn();
        let id = make_block(&conn, 2); // 2 小时后的提醒：未来时点
        conn.execute(
            "UPDATE todo_blocks SET reminder_ack_at = ?2, due_at = ?3 WHERE id = ?1",
            rusqlite::params![id, iso_from_now(-1), iso_from_now(-30i64 / 60 * 60 / 60 * 60)],
        )
        .unwrap();
        // 上面的 due_at 表达式等价于 -0.5h，直接改写成清晰值：
        conn.execute(
            "UPDATE todo_blocks SET due_at = ?2 WHERE id = ?1",
            rusqlite::params![id, iso_from_now(-1)],
        )
        .unwrap();

        let (_, next) = collect_pending(&conn).unwrap();
        assert!(next.is_some(), "未来提醒应参与睡眠计算");
        let due_list = collect_pending(&conn).unwrap().0;
        assert_eq!(due_list.len(), 1, "仅截止到期触发，未来提醒不触发");
        assert_eq!(due_list[0].1, ReminderKind::Due);
        assert_eq!(due_list[0].0.id, id);
    }
}
