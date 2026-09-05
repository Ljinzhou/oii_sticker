//! SQLite schema 初始化与迁移。
//!
//! 使用 SQLite 原生的 `PRAGMA user_version` 整数字段保存 schema 版本号：
//! - 无需自建迁移表；
//! - 首次启动版本为 0，调用 `init_schema` 后写入最新版本号；
//! - 升级版本时按 `if current < N` 分支补充迁移脚本。

use anyhow::{Context, Result};
use rusqlite::Connection;

/// 目标 schema 版本号。新增迁移时同步递增此常量。
pub const SCHEMA_VERSION: u32 = 18;

/// 首次启动（DB 为空）时建表并写入默认配置。
pub fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA_SQL)
        .context("创建初始 schema 失败")?;

    // 写入 system_config 默认条目。
    let defaults: &[(&str, &str, &str)] = &[
        ("auto_scroll_speed", "30", "自动滚动速度 px/s"),
        ("flash_speed", "800", "提醒闪烁间隔 ms"),
        ("assets_dir", "", "资源目录，空则用默认"),
        ("db_path", "", "数据库路径，空则用默认"),
        ("edit_inactivity_timeout", "5000", "编辑模式无操作退出 ms"),
        ("edit_mode_granularity", "global", "编辑模式切换粒度"),
        // v2：便签窗口默认偏好。
        ("default_sticker_opacity", "0.9", "新便签默认背景透明度"),
        ("default_sticker_title_centered", "0", "新便签默认标题是否居中（0 居左，1 居中）"),
        ("default_sticker_title_font_size", "14", "新便签默认标题字号（px）"),
        ("default_sticker_body_font_size", "13", "新便签默认正文字号（px）"),
        ("default_sticker_bg_color", "#FFF4D6", "新便签默认背景颜色（#RRGGBB）"),
        ("default_sticker_text_color", "#222222", "新便签默认正文颜色（#RRGGBB）"),
        // v3：自动滚动默认参数。
        ("default_sticker_auto_scroll", "0", "新便签默认是否自动滚动正文（0 关，1 开）"),
        ("default_sticker_auto_scroll_speed", "30", "新便签默认自动滚动速度（px/s）"),
        // v4：窗口置顶默认参数。
        ("default_sticker_always_on_top", "1", "新便签默认是否置顶（0 否，1 是）"),
        ("default_todo_always_on_top", "1", "Todo 窗口默认是否置顶"),
        ("recent_slash_commands", "[]", "最近使用的斜杠命令 JSON 数组"),
        ("todo_remind_tomorrow_hour", "9", "Todo 明天提醒的小时"),
        ("todo_remind_next_week_dow", "1", "Todo 下周提醒星期（0=周日）"),
        ("todo_remind_next_week_hour", "9", "Todo 下周提醒的小时"),
        ("todo_due_today_hour", "18", "Todo 今天截止的小时"),
        ("todo_due_tomorrow_hour", "9", "Todo 明天截止的小时"),
        ("todo_due_next_week_dow", "1", "Todo 下周截止星期（0=周日）"),
    ];

    let mut stmt = conn
        .prepare(
            "INSERT OR IGNORE INTO system_config (key, value, description)
             VALUES (?1, ?2, ?3)",
        )
        .context("准备 system_config 插入语句失败")?;
    for (k, v, desc) in defaults {
        stmt.execute(rusqlite::params![k, v, desc])
            .with_context(|| format!("插入默认配置 {k} 失败"))?;
    }

    // 标记 schema 版本为最新（与 SCHEMA_VERSION 保持一致，避免漂移）。
    conn.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};"))
        .context("设置 user_version 失败")?;
    Ok(())
}

/// 在一个 IMMEDIATE 事务里执行迁移闭包：成功提交，失败回滚。
/// 保证"任意时刻中断，重启后迁移可安全重跑"——要么全部生效，要么全部不生效。
fn in_tx(conn: &Connection, f: impl FnOnce(&Connection) -> Result<()>) -> Result<()> {
    conn.execute_batch("BEGIN IMMEDIATE;")
        .context("开启迁移事务失败")?;
    match f(conn) {
        Ok(()) => {
            conn.execute_batch("COMMIT;")
                .context("提交迁移事务失败")?;
            Ok(())
        }
        Err(e) => {
            // 回滚失败不掩盖原始错误。
            let _ = conn.execute_batch("ROLLBACK;");
            Err(e)
        }
    }
}

/// 判断表是否已包含某列（ALTER 前检查，保证迁移幂等）。
fn table_has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .with_context(|| format!("读取表 {table} 结构失败"))?;
    let names = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .context("枚举表列失败")?;
    for name in names {
        if name? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

/// v1 → v2 迁移：新增 sticker_prefs 表。
fn migrate_v1_to_v2(conn: &Connection) -> Result<()> {
    in_tx(conn, |c| {
        c.execute_batch(
            "CREATE TABLE IF NOT EXISTS sticker_prefs (
                sticker_id        INTEGER PRIMARY KEY REFERENCES stickers(id) ON DELETE CASCADE,
                opacity           REAL,
                title_centered    INTEGER,
                title_font_size   INTEGER,
                body_font_size    INTEGER,
                bg_color          TEXT,
                text_color        TEXT
            );

            INSERT OR IGNORE INTO system_config (key, value, description) VALUES
                ('default_sticker_opacity',         '0.9',    '新便签默认背景透明度'),
                ('default_sticker_title_centered',  '0',      '新便签默认标题是否居中（0 居左，1 居中）'),
                ('default_sticker_title_font_size', '14',     '新便签默认标题字号（px）'),
                ('default_sticker_body_font_size',  '13',     '新便签默认正文字号（px）'),
                ('default_sticker_bg_color',        '#FFF4D6','新便签默认背景颜色（#RRGGBB）'),
                ('default_sticker_text_color',      '#222222','新便签默认正文颜色（#RRGGBB）');
            ",
        )
        .context("迁移 v1→v2 失败")?;
        Ok(())
    })
}

/// v2 → v3 迁移：sticker_prefs 加 auto_scroll_speed 列；system_config 加自动滚动默认键。
fn migrate_v2_to_v3(conn: &Connection) -> Result<()> {
    in_tx(conn, |c| {
        // ALTER 前检查列存在，保证中断重跑不因 duplicate column 失败。
        if !table_has_column(c, "sticker_prefs", "auto_scroll_speed")? {
            c.execute_batch("ALTER TABLE sticker_prefs ADD COLUMN auto_scroll_speed INTEGER;")
                .context("迁移 v2→v3：加列失败")?;
        }
        c.execute_batch(
            "INSERT OR IGNORE INTO system_config (key, value, description) VALUES
                ('default_sticker_auto_scroll',        '0',  '新便签默认是否自动滚动正文（0 关，1 开）'),
                ('default_sticker_auto_scroll_speed',  '30', '新便签默认自动滚动速度（px/s）');
            ",
        )
        .context("迁移 v2→v3：写入默认配置失败")?;
        Ok(())
    })
}

/// v3 → v4 迁移：system_config 加"窗口置顶"默认键。
fn migrate_v3_to_v4(conn: &Connection) -> Result<()> {
    in_tx(conn, |c| {
        c.execute_batch(
            "INSERT OR IGNORE INTO system_config (key, value, description) VALUES
                ('default_sticker_always_on_top',  '1',  '新便签默认是否置顶（0 否，1 是）');
            ",
        )
        .context("迁移 v3→v4 失败")?;
        Ok(())
    })
}

/// v5 → v6 迁移：默认背景色统一为 #FFF4D6（旧默认 #CCFFCC 修正，
/// 与便签实际默认背景保持一致）。
fn migrate_v5_to_v6(conn: &Connection) -> Result<()> {
    in_tx(conn, |c| {
        c.execute(
            "UPDATE system_config SET value = '#FFF4D6'
             WHERE key = 'default_sticker_bg_color' AND value = '#CCFFCC'",
            [],
        )
        .context("迁移 v5→v6 失败")?;
        Ok(())
    })
}

/// v6 → v7：新增独立 Todo 块表，不影响旧 todo_items。
fn migrate_v6_to_v7(conn: &Connection) -> Result<()> {
    in_tx(conn, |c| {
        c.execute_batch(
            "CREATE TABLE IF NOT EXISTS todo_blocks (
                id TEXT PRIMARY KEY,
                sticker_id INTEGER NOT NULL REFERENCES stickers(id) ON DELETE CASCADE,
                title TEXT NOT NULL DEFAULT '',
                description TEXT,
                is_completed INTEGER NOT NULL DEFAULT 0,
                parent_id TEXT REFERENCES todo_blocks(id) ON DELETE CASCADE,
                reminder_at TEXT,
                due_at TEXT,
                repeat_rule TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_todo_blocks_parent ON todo_blocks(parent_id);
            CREATE INDEX IF NOT EXISTS idx_todo_blocks_sticker ON todo_blocks(sticker_id);
            INSERT OR IGNORE INTO system_config (key, value, description) VALUES
                ('recent_slash_commands', '[]', '最近使用的斜杠命令 JSON 数组'),
                ('todo_remind_tomorrow_hour', '9', 'Todo 明天提醒的小时'),
                ('todo_remind_next_week_dow', '1', 'Todo 下周提醒星期（0=周日）'),
                ('todo_remind_next_week_hour', '9', 'Todo 下周提醒的小时'),
                ('todo_due_today_hour', '18', 'Todo 今天截止的小时'),
                ('todo_due_tomorrow_hour', '9', 'Todo 明天截止的小时'),
                ('todo_due_next_week_dow', '1', 'Todo 下周截止星期（0=周日）');",
        )
        .context("迁移 v6→v7 失败")?;
        Ok(())
    })
}

/// v7 → v8：新增 Todo 窗口默认置顶配置，不覆盖已有用户值。
fn migrate_v7_to_v8(conn: &Connection) -> Result<()> {
    in_tx(conn, |c| {
        c.execute(
            "INSERT OR IGNORE INTO system_config (key, value, description)
             VALUES ('default_todo_always_on_top', '1', 'Todo 窗口默认是否置顶')",
            [],
        )
        .context("迁移 v7→v8 失败")?;
        Ok(())
    })
}

/// v8 → v9：Todo 块新增独立「块标题」（block_title），
/// 卡头不再直接使用第一个任务名作为标题。
fn migrate_v8_to_v9(conn: &Connection) -> Result<()> {
    in_tx(conn, |c| {
        let has_table: bool = c.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'todo_blocks')",
            [],
            |row| row.get(0),
        )?;
        if has_table && !has_column(c, "todo_blocks", "block_title")? {
            c.execute(
                "ALTER TABLE todo_blocks ADD COLUMN block_title TEXT NOT NULL DEFAULT ''",
                [],
            )
            .context("迁移 v8→v9 失败")?;
        }
        Ok(())
    })
}

/// 表是否已含指定列（迁移幂等检查）。
fn has_column(conn: &Connection, table: &str, col: &str) -> Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let cols = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(cols.iter().any(|c| c == col))
}

/// v9 → v10：Todo 块新增 `sort_order`，支持任务拖拽排序（旧数据保持创建顺序）。
fn migrate_v9_to_v10(conn: &Connection) -> Result<()> {
    in_tx(conn, |c| {
        let has_table: bool = c.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'todo_blocks')",
            [],
            |row| row.get(0),
        )?;
        if has_table && !has_column(c, "todo_blocks", "sort_order")? {
            c.execute(
                "ALTER TABLE todo_blocks ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0",
                [],
            )
            .context("迁移 v9→v10 失败")?;
        }
        Ok(())
    })
}

/// v10 → v11：新增 file_history 表（记事本替代功能阅读历史预留）。
/// CREATE TABLE IF NOT EXISTS 天然幂等，无需列级检查。
fn migrate_v10_to_v11(conn: &Connection) -> Result<()> {
    in_tx(conn, |c| {
        c.execute_batch(
            "CREATE TABLE IF NOT EXISTS file_history (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                path            TEXT NOT NULL,
                name            TEXT NOT NULL,
                size            INTEGER NOT NULL DEFAULT 0,
                last_opened_at  TEXT NOT NULL DEFAULT (datetime('now')),
                open_count      INTEGER NOT NULL DEFAULT 1,
                archived        INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_file_history_path ON file_history(path);
            ",
        )
        .context("迁移 v10→v11 失败")?;
        Ok(())
    })
}

/// v11 → v12：便签分组表 + stickers.group_id（NULL = 未分组，无内置默认分组）。
fn migrate_v11_to_v12(conn: &Connection) -> Result<()> {
    in_tx(conn, |c| {
        c.execute_batch(
            "CREATE TABLE IF NOT EXISTS sticker_groups (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                name       TEXT NOT NULL,
                sort_order INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )
        .context("迁移 v11→v12 创建 sticker_groups 失败")?;
        let has_table: bool = c.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'stickers')",
            [],
            |row| row.get(0),
        )?;
        if has_table && !has_column(c, "stickers", "group_id")? {
            c.execute_batch(
                "ALTER TABLE stickers ADD COLUMN group_id INTEGER REFERENCES sticker_groups(id) ON DELETE SET NULL;",
            )
            .context("迁移 v11→v12 失败")?;
        }
        Ok(())
    })
}

/// v12 → v13 迁移：移除便签提醒功能（sticker_attrs 表 + stickers.alert_active 列）。
fn migrate_v12_to_v13(conn: &Connection) -> Result<()> {
    in_tx(conn, |c| {
        c.execute_batch("DROP TABLE IF EXISTS sticker_attrs;")
            .context("迁移 v12→v13 删除 sticker_attrs 失败")?;
        if table_has_column(c, "stickers", "alert_active")? {
            c.execute_batch("ALTER TABLE stickers DROP COLUMN alert_active;")
                .context("迁移 v12→v13 删除 alert_active 列失败")?;
        }
        Ok(())
    })
}

/// v13 → v14 迁移：新增 stickers.window_hidden 列（启动恢复显示/隐藏状态）。
/// 兼容没有 stickers 表的合成库（部分迁移单测只建了零散表），有表才加列。
fn migrate_v13_to_v14(conn: &Connection) -> Result<()> {
    in_tx(conn, |c| {
        let has_table: bool = c.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'stickers')",
            [],
            |r| r.get(0),
        )?;
        if has_table && !table_has_column(c, "stickers", "window_hidden")? {
            c.execute_batch(
                "ALTER TABLE stickers ADD COLUMN window_hidden INTEGER NOT NULL DEFAULT 0;",
            )
            .context("迁移 v13→v14 新增 window_hidden 列失败")?;
        }
        Ok(())
    })
}

/// v14 → v15 迁移：Todo 提醒触发状态列。
///
/// `reminded_at` / `due_notified_at` 分别记录 reminder_at / due_at 最近一次
/// 已触发提醒的时间；调度器据此避免同一时点重复触发。兼容没有 todo_blocks
/// 表的合成库（部分迁移单测只建了零散表），有表才加列。
fn migrate_v14_to_v15(conn: &Connection) -> Result<()> {
    in_tx(conn, |c| {
        let has_table: bool = c.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'todo_blocks')",
            [],
            |r| r.get(0),
        )?;
        if has_table {
            if !table_has_column(c, "todo_blocks", "reminded_at")? {
                c.execute_batch("ALTER TABLE todo_blocks ADD COLUMN reminded_at TEXT;")
                    .context("迁移 v14→v15 新增 reminded_at 列失败")?;
            }
            if !table_has_column(c, "todo_blocks", "due_notified_at")? {
                c.execute_batch("ALTER TABLE todo_blocks ADD COLUMN due_notified_at TEXT;")
                    .context("迁移 v14→v15 新增 due_notified_at 列失败")?;
            }
        }
        Ok(())
    })
}

/// v15 → v16 迁移：Todo 提醒确认列。
///
/// 用户点击红点确认收到提醒后写入对应确认时刻：高亮立即消失，
/// 且调度器不再为「不晚于确认时刻」的同字段提醒重复弹通知
/// （重启后也不会再提示）；之后新设置的提醒/循环任务后续周期照常触发。
fn migrate_v15_to_v16(conn: &Connection) -> Result<()> {
    in_tx(conn, |c| {
        let has_table: bool = c.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'todo_blocks')",
            [],
            |r| r.get(0),
        )?;
        if has_table {
            if !table_has_column(c, "todo_blocks", "reminder_ack_at")? {
                c.execute_batch("ALTER TABLE todo_blocks ADD COLUMN reminder_ack_at TEXT;")
                    .context("迁移 v15→v16 新增 reminder_ack_at 列失败")?;
            }
            if !table_has_column(c, "todo_blocks", "due_ack_at")? {
                c.execute_batch("ALTER TABLE todo_blocks ADD COLUMN due_ack_at TEXT;")
                    .context("迁移 v15→v16 新增 due_ack_at 列失败")?;
            }
        }
        Ok(())
    })
}

/// v16 → v17 迁移：Todo 任务完成时刻列（completed_at）。
///
/// 「已完成任务块」在便签中展示每项任务的完成时间；
/// 完成翻转由 update 写入本地时间，取消完成时清空。
fn migrate_v16_to_v17(conn: &Connection) -> Result<()> {
    in_tx(conn, |c| {
        let has_table: bool = c.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'todo_blocks')",
            [],
            |r| r.get(0),
        )?;
        if has_table && !table_has_column(c, "todo_blocks", "completed_at")? {
            c.execute_batch("ALTER TABLE todo_blocks ADD COLUMN completed_at TEXT;")
                .context("迁移 v16→v17 新增 completed_at 列失败")?;
        }
        Ok(())
    })
}

/// v17 → v18 迁移：Todo 重复任务重建锚点列（repeat_anchor）。
///
/// 「每天/每周…」重复任务由调度线程每日零点重建：周期结束未完成的任务
/// 标题追加「——YYYY年M月D日，任务逾期」并自动新建同名任务。锚点列记录
/// 下一次应重建的本地日期，NULL 表示不参与重建队列。
fn migrate_v17_to_v18(conn: &Connection) -> Result<()> {
    in_tx(conn, |c| {
        let has_table: bool = c.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'todo_blocks')",
            [],
            |r| r.get(0),
        )?;
        if has_table && !table_has_column(c, "todo_blocks", "repeat_anchor")? {
            c.execute_batch("ALTER TABLE todo_blocks ADD COLUMN repeat_anchor TEXT;")
                .context("迁移 v17→v18 新增 repeat_anchor 列失败")?;
        }
        Ok(())
    })
}

/// 把现有数据库迁移到最新 schema 版本。
///
/// 幂等：对已是最新的 DB 是 no-op，仅在版本落后时执行迁移分支。
pub fn run_migrations(conn: &Connection) -> Result<u32> {
    let current: u32 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .context("读取 user_version 失败")?;

    if current == 0 {
        // 全新数据库，走首次安装路径。
        init_schema(conn)?;
        return Ok(SCHEMA_VERSION);
    }

    if current < 2 {
        migrate_v1_to_v2(conn)?;
    }
    if current < 3 {
        migrate_v2_to_v3(conn)?;
    }
    if current < 4 {
        migrate_v3_to_v4(conn)?;
    }
    if current < 6 {
        migrate_v5_to_v6(conn)?;
    }
    if current < 7 {
        migrate_v6_to_v7(conn)?;
    }

    if current < 8 {
        migrate_v7_to_v8(conn)?;
    }

    if current < 9 {
        migrate_v8_to_v9(conn)?;
    }

    if current < 10 {
        migrate_v9_to_v10(conn)?;
    }

    if current < 11 {
        migrate_v10_to_v11(conn)?;
    }

    if current < 12 {
        migrate_v11_to_v12(conn)?;
    }

    if current < 13 {
        migrate_v12_to_v13(conn)?;
    }

    if current < 14 {
        migrate_v13_to_v14(conn)?;
    }

    if current < 15 {
        migrate_v14_to_v15(conn)?;
    }

    if current < 16 {
        migrate_v15_to_v16(conn)?;
    }

    if current < 17 {
        migrate_v16_to_v17(conn)?;
    }

    if current < 18 {
        migrate_v17_to_v18(conn)?;
    }

    // 升级完成后把 user_version 写到位。
    conn.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};"))
        .context("更新 user_version 失败")?;

    Ok(SCHEMA_VERSION)
}

/// SQL DDL，由 `init_schema` 执行。独立成常量方便 plan / 文档工具复用。
const SCHEMA_SQL: &str = include_str!("schema.sql");

#[cfg(test)]
mod tests {
    use super::*;

    /// v12 库执行迁移：sticker_attrs 表与 stickers.alert_active 列应被移除。
    #[test]
    fn migrate_v12_to_v13_drops_reminder_artifacts() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA_SQL).unwrap();
        // 模拟 v12 状态：重新补上被移除的提醒表/列，版本调回 12。
        conn.execute_batch(
            "CREATE TABLE sticker_attrs (
                sticker_id INTEGER PRIMARY KEY REFERENCES stickers(id) ON DELETE CASCADE,
                due_date TEXT,
                remind_at TEXT,
                remind_rule TEXT,
                is_recurring INTEGER NOT NULL DEFAULT 0
             );
             ALTER TABLE stickers ADD COLUMN alert_active INTEGER NOT NULL DEFAULT 0;
             PRAGMA user_version = 12;",
        )
        .unwrap();

        assert_eq!(run_migrations(&conn).unwrap(), SCHEMA_VERSION);

        let attrs_cnt: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                  WHERE type = 'table' AND name = 'sticker_attrs'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(attrs_cnt, 0);
        assert!(!table_has_column(&conn, "stickers", "alert_active").unwrap());

        // 重跑幂等
        assert_eq!(run_migrations(&conn).unwrap(), SCHEMA_VERSION);
    }

    /// v13 库执行迁移：window_hidden 列应被补上且默认 0（显示）；重跑幂等。
    #[test]
    fn migrate_v13_to_v14_adds_window_hidden() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA_SQL).unwrap();
        // 模拟 v13 状态：去掉新列，版本调回 13。
        conn.execute_batch(
            "ALTER TABLE stickers DROP COLUMN window_hidden;
             PRAGMA user_version = 13;",
        )
        .unwrap();

        assert_eq!(run_migrations(&conn).unwrap(), SCHEMA_VERSION);
        assert!(table_has_column(&conn, "stickers", "window_hidden").unwrap());

        conn.execute_batch("INSERT INTO stickers (title) VALUES ('恢复用');")
            .unwrap();
        let hidden: i64 = conn
            .query_row("SELECT window_hidden FROM stickers", [], |r| r.get(0))
            .unwrap();
        assert_eq!(hidden, 0, "新列默认 0（显示）");

        // 重跑幂等
        assert_eq!(run_migrations(&conn).unwrap(), SCHEMA_VERSION);
    }

    /// v15 库执行迁移：todo_blocks 应补上提醒确认列；重跑幂等。
    #[test]
    fn migrate_v15_to_v16_adds_ack_columns() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA_SQL).unwrap();
        conn.execute_batch(
            "ALTER TABLE todo_blocks DROP COLUMN reminder_ack_at;
             ALTER TABLE todo_blocks DROP COLUMN due_ack_at;
             PRAGMA user_version = 15;",
        )
        .unwrap();

        assert_eq!(run_migrations(&conn).unwrap(), SCHEMA_VERSION);
        assert!(table_has_column(&conn, "todo_blocks", "reminder_ack_at").unwrap());
        assert!(table_has_column(&conn, "todo_blocks", "due_ack_at").unwrap());

        // 重跑幂等
        assert_eq!(run_migrations(&conn).unwrap(), SCHEMA_VERSION);
    }

    /// v16 库执行迁移：todo_blocks 应补上完成时刻列 completed_at；重跑幂等。
    #[test]
    fn migrate_v16_to_v17_adds_completed_at_column() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA_SQL).unwrap();
        // 模拟 v16 状态：去掉新列，版本调回 16。
        conn.execute_batch(
            "ALTER TABLE todo_blocks DROP COLUMN completed_at;
             PRAGMA user_version = 16;",
        )
        .unwrap();

        assert_eq!(run_migrations(&conn).unwrap(), SCHEMA_VERSION);
        assert!(table_has_column(&conn, "todo_blocks", "completed_at").unwrap());

        // 重跑幂等
        assert_eq!(run_migrations(&conn).unwrap(), SCHEMA_VERSION);
    }

    /// v14 库执行迁移：todo_blocks 应补上提醒触发状态列；重跑幂等。
    #[test]
    fn migrate_v14_to_v15_adds_reminder_flag_columns() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA_SQL).unwrap();
        // 模拟 v14 状态：去掉新列，版本调回 14。
        conn.execute_batch(
            "ALTER TABLE todo_blocks DROP COLUMN reminded_at;
             ALTER TABLE todo_blocks DROP COLUMN due_notified_at;
             PRAGMA user_version = 14;",
        )
        .unwrap();

        assert_eq!(run_migrations(&conn).unwrap(), SCHEMA_VERSION);
        assert!(table_has_column(&conn, "todo_blocks", "reminded_at").unwrap());
        assert!(table_has_column(&conn, "todo_blocks", "due_notified_at").unwrap());

        // 重跑幂等
        assert_eq!(run_migrations(&conn).unwrap(), SCHEMA_VERSION);
    }

    /// v2 库执行迁移：auto_scroll_speed 列应被补上；
    /// 迁移中断重跑（直接重复调用 migrate_v2_to_v3）不报错。
    #[test]
    fn migrate_v2_to_v3_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA_SQL).unwrap();
        // 模拟 v2 状态：删掉 v3 加的列，版本调回 2。
        conn.execute_batch(
            "ALTER TABLE sticker_prefs DROP COLUMN auto_scroll_speed;
             PRAGMA user_version = 2;",
        )
        .unwrap();

        // 第一次迁移到 v6。
        assert_eq!(run_migrations(&conn).unwrap(), SCHEMA_VERSION);

        // 模拟"迁移中途崩溃后重跑"：列已存在时再跑 v2→v3 不得报错。
        migrate_v2_to_v3(&conn).unwrap();

        // 再次全量迁移应为 no-op 成功。
        assert_eq!(run_migrations(&conn).unwrap(), SCHEMA_VERSION);

        // 列确实存在。
        assert!(table_has_column(&conn, "sticker_prefs", "auto_scroll_speed").unwrap());
    }

    /// 迁移失败必须回滚：v2→v3 中后续语句失败时，
    /// 连接不得残留未提交事务状态。
    #[test]
    fn migrate_failure_rolls_back() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA_SQL).unwrap();
        // 模拟 v2 状态：schema.sql 不写 user_version，需手动调回 2；
        // 否则最后的 run_migrations 会误走 init_schema（current==0）分支。
        conn.execute_batch("PRAGMA user_version = 2;").unwrap();
        // 制造失败：把表改名，让 INSERT 语句因表不存在而失败。
        conn.execute_batch("ALTER TABLE sticker_prefs RENAME TO sticker_prefs_bak;")
            .unwrap();

        let err = migrate_v2_to_v3(&conn);
        assert!(err.is_err(), "v2→v3 在 sticker_prefs 缺失时应失败");

        // 事务已回滚：连接不在事务中，可以继续执行新语句。
        conn.execute_batch("ALTER TABLE sticker_prefs_bak RENAME TO sticker_prefs;")
            .unwrap();
        assert_eq!(run_migrations(&conn).unwrap(), SCHEMA_VERSION);
    }

    /// 全新库：init_schema 建全部表并写 user_version=5。
    #[test]
    fn init_schema_creates_all_tables() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        let tables: Vec<String> = conn
            .prepare(
                "SELECT name FROM sqlite_master
                  WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
            )
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(
            tables,
            vec![
                "assets",
                "completion_log",
                "file_history",
                "sticker_groups",
                "sticker_prefs",
                "stickers",
                "system_config",
                "todo_blocks",
                "todo_items",
            ]
        );
        let v: u32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, SCHEMA_VERSION);
    }

    /// v5 → v6：旧默认背景色 #CCFFCC 迁移为 #FFF4D6，其他键不受影响。
    #[test]
    fn migrate_v5_to_v6_fixes_bg_color_default() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE system_config (key TEXT PRIMARY KEY, value TEXT NOT NULL, description TEXT);
             INSERT INTO system_config VALUES ('default_sticker_bg_color', '#CCFFCC', 'x');
             INSERT INTO system_config VALUES ('default_sticker_opacity', '0.9', 'x');",
        )
        .unwrap();
        migrate_v5_to_v6(&conn).unwrap();
        let v: String = conn
            .query_row(
                "SELECT value FROM system_config WHERE key = 'default_sticker_bg_color'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(v, "#FFF4D6");
        let custom: String = conn
            .query_row(
                "SELECT value FROM system_config WHERE key = 'default_sticker_opacity'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(custom, "0.9", "其他键不受影响");
    }

    #[test]
    fn migrate_v6_to_v7_creates_todo_blocks_for_existing_database() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE stickers (id INTEGER PRIMARY KEY);
             CREATE TABLE system_config (key TEXT PRIMARY KEY, value TEXT NOT NULL, description TEXT NOT NULL DEFAULT '', updated_at TEXT);
             PRAGMA user_version = 6;",
        ).unwrap();
        assert_eq!(run_migrations(&conn).unwrap(), SCHEMA_VERSION);
        let exists: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='todo_blocks')", [], |row| row.get(0)).unwrap();
        assert!(exists);
        assert_eq!(
            conn.query_row("SELECT value FROM system_config WHERE key='recent_slash_commands'", [], |row| row.get::<_, String>(0)).unwrap(),
            "[]",
        );
        assert_eq!(
            conn.query_row(
                "SELECT value FROM system_config WHERE key='default_todo_always_on_top'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
            "1",
        );
    }

    #[test]
    fn migrate_v8_to_v9_adds_block_title_column() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE stickers (id INTEGER PRIMARY KEY);
             CREATE TABLE todo_blocks (id TEXT PRIMARY KEY, sticker_id INTEGER);
             PRAGMA user_version = 8;",
        )
        .unwrap();
        assert_eq!(run_migrations(&conn).unwrap(), SCHEMA_VERSION);
        assert!(table_has_column(&conn, "todo_blocks", "block_title").unwrap());
    }

    #[test]
    fn migrate_v7_to_v8_adds_todo_window_topmost_default() {
        let existing = Connection::open_in_memory().unwrap();
        existing
            .execute_batch(
                "CREATE TABLE system_config (key TEXT PRIMARY KEY, value TEXT NOT NULL, description TEXT);
                 INSERT INTO system_config VALUES ('default_todo_always_on_top', '0', '用户值');
                 PRAGMA user_version = 7;",
            )
            .unwrap();

        assert_eq!(run_migrations(&existing).unwrap(), SCHEMA_VERSION);
        assert_eq!(
            existing
                .query_row(
                    "SELECT value FROM system_config WHERE key = 'default_todo_always_on_top'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "0",
            "迁移不得覆盖已有用户值",
        );

        let missing = Connection::open_in_memory().unwrap();
        missing
            .execute_batch(
                "CREATE TABLE system_config (key TEXT PRIMARY KEY, value TEXT NOT NULL, description TEXT);
                 PRAGMA user_version = 7;",
            )
            .unwrap();

        assert_eq!(run_migrations(&missing).unwrap(), SCHEMA_VERSION);
        assert_eq!(
            missing
                .query_row(
                    "SELECT value FROM system_config WHERE key = 'default_todo_always_on_top'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "1",
        );
    }

    /// v10 库执行迁移：file_history 表应被创建；重复执行幂等。
    #[test]
    fn migrate_v10_to_v11_creates_file_history_idempotently() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA user_version = 10;").unwrap();

        assert_eq!(run_migrations(&conn).unwrap(), SCHEMA_VERSION);
        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='file_history')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(exists);
        let cols: Vec<String> = conn
            .prepare("SELECT name FROM pragma_table_info('file_history') ORDER BY cid")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(cols, vec!["id", "path", "name", "size", "last_opened_at", "open_count", "archived"]);

        // 重复迁移不得报错（CREATE TABLE IF NOT EXISTS 幂等）。
        assert_eq!(run_migrations(&conn).unwrap(), SCHEMA_VERSION);
    }
}
