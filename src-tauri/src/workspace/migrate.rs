//! 旧库（%APPDATA% 的 stickers.db）→ 当前工作空间库的全量迁移。

use anyhow::{Context, Result};
use rusqlite::{params, Connection};

/// 迁移结果摘要。
#[derive(Debug, Default, Clone)]
pub struct MigrateSummary {
    pub stickers: usize,
    pub todo_items: usize,
    pub todo_blocks: usize,
    pub prefs: usize,
    pub attrs: usize,
}

/// 以只读方式打开旧库（不存在则跳过）。
fn open_legacy(path: &std::path::Path) -> Result<Option<Connection>> {
    if !path.exists() {
        return Ok(None);
    }
    let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .context("打开旧数据库失败")?;
    Ok(Some(conn))
}

/// 迁移 `legacy_db_path` 全部数据进 `new_conn`（新库已迁移 schema）。
/// 便签 content 写入 md 文件（new_root 根目录）；元数据 + todo/prefs/config 入新库。
pub fn run(
    legacy_db_path: &std::path::Path,
    new_conn: &Connection,
    new_root: &std::path::Path,
) -> Result<MigrateSummary> {
    let Some(legacy) = open_legacy(legacy_db_path)? else {
        return Ok(MigrateSummary::default());
    };
    // 1) stickers：for 每行 SELECT 全字段 → 写 md + INSERT（原 id 保留, content 留空）
    let mut stmt = legacy.prepare(
        "SELECT id, parent_id, title, content, heading_level, pos_x, pos_y, width, height,
                opacity, bg_color, always_on_top, auto_scroll, is_completed, alert_active,
                display_mode, created_at, updated_at FROM stickers ORDER BY id",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, i64>(8)?,
                row.get::<_, f64>(9)?,
                row.get::<_, Option<String>>(10)?,
                row.get::<_, i64>(11)?,
                row.get::<_, i64>(12)?,
                row.get::<_, i64>(13)?,
                row.get::<_, i64>(14)?,
                row.get::<_, String>(15)?,
                row.get::<_, String>(16)?,
                row.get::<_, String>(17)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut summary = MigrateSummary::default();
    // 1) md 文件先行（事务外）：文件落盘失败时提前返回，DB 尚未写入。
    for (id, _parent_id, title, content, ..) in &rows {
        let file_name = crate::workspace::layout::sticker_file_name(*id, title);
        crate::workspace::md_store::write(new_root, &file_name, content)?;
    }
    // 2) 全量 DB 插入放入同一事务：任一阶段失败整体回滚，不留半迁移状态。
    let tx = new_conn.unchecked_transaction().context("开启迁移事务失败")?;
    for (
        id,
        parent_id,
        title,
        _content,
        heading_level,
        pos_x,
        pos_y,
        width,
        height,
        opacity,
        bg_color,
        always_on_top,
        auto_scroll,
        is_completed,
        alert_active,
        display_mode,
        created_at,
        updated_at,
    ) in &rows
    {
        tx.execute(
            "INSERT OR IGNORE INTO stickers
               (id, parent_id, title, content, heading_level, pos_x, pos_y, width, height, opacity, bg_color,
                always_on_top, auto_scroll, is_completed, alert_active, display_mode, created_at, updated_at)
             VALUES (?1,?2,?3,'',?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
            params![id, parent_id, title, heading_level, pos_x, pos_y, width, height, opacity, bg_color,
                    always_on_top, auto_scroll, is_completed, alert_active, display_mode, created_at, updated_at],
        ).context("迁移便签失败")?;
        summary.stickers += 1;
    }
    // 3) 其余表固定列复制（同一事务内）。
    const TODO_ITEM_COLS: &[&str] = &[
        "id",
        "sticker_id",
        "child_sticker_id",
        "text",
        "done",
        "completed_at",
        "sort_order",
        "due_date",
        "remind_at",
        "remind_rule",
        "is_recurring",
    ];
    const TODO_BLOCK_COLS: &[&str] = &[
        "id",
        "sticker_id",
        "title",
        "block_title",
        "description",
        "is_completed",
        "parent_id",
        "reminder_at",
        "due_at",
        "repeat_rule",
        "sort_order",
        "created_at",
        "updated_at",
    ];
    const PREFS_COLS: &[&str] = &[
        "sticker_id",
        "opacity",
        "title_centered",
        "title_font_size",
        "body_font_size",
        "bg_color",
        "text_color",
        "auto_scroll_speed",
    ];
    const ATTRS_COLS: &[&str] = &[
        "sticker_id",
        "due_date",
        "remind_at",
        "remind_rule",
        "is_recurring",
    ];
    copy_fixed(&legacy, &tx, "todo_items", TODO_ITEM_COLS)?;
    copy_fixed(&legacy, &tx, "todo_blocks", TODO_BLOCK_COLS)?;
    copy_fixed(&legacy, &tx, "sticker_prefs", PREFS_COLS)?;
    copy_fixed(&legacy, &tx, "sticker_attrs", ATTRS_COLS)?;
    tx.commit().context("提交迁移事务失败")?;
    summary.todo_items = count_of(&legacy, "todo_items");
    summary.todo_blocks = count_of(&legacy, "todo_blocks");
    summary.prefs = count_of(&legacy, "sticker_prefs");
    summary.attrs = count_of(&legacy, "sticker_attrs");
    Ok(summary)
}

/// 固定列复制：列清单与 VALUES 占位符一一对应（?1..?N），ValueRef 直映射无损保真类型。
fn copy_fixed(legacy: &Connection, new: &Connection, table: &str, cols: &[&str]) -> Result<usize> {
    let mut stmt = legacy.prepare(&format!(
        "SELECT {} FROM {table} ORDER BY rowid",
        cols.join(",")
    ))?;
    let rows = stmt
        .query_map([], |row| {
            let mut vals = Vec::with_capacity(cols.len());
            for i in 0..cols.len() {
                vals.push(match row.get_ref(i)? {
                    rusqlite::types::ValueRef::Integer(v) => rusqlite::types::Value::Integer(v),
                    rusqlite::types::ValueRef::Real(v) => rusqlite::types::Value::Real(v),
                    rusqlite::types::ValueRef::Text(v) => {
                        rusqlite::types::Value::Text(String::from_utf8_lossy(v).into_owned())
                    }
                    rusqlite::types::ValueRef::Blob(v) => rusqlite::types::Value::Blob(v.to_vec()),
                    rusqlite::types::ValueRef::Null => rusqlite::types::Value::Null,
                });
            }
            Ok(vals)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let placeholders = (1..=cols.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "INSERT OR IGNORE INTO {table} ({}) VALUES ({placeholders})",
        cols.join(",")
    );
    let mut n = 0usize;
    for vals in rows {
        new.execute(&sql, rusqlite::params_from_iter(vals))?;
        n += 1;
    }
    Ok(n)
}

fn count_of(conn: &Connection, table: &str) -> usize {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| {
        r.get::<_, i64>(0)
    })
    .unwrap_or(0) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema;

    #[test]
    fn migrates_stickers_with_md_files_and_keeps_ids() {
        let dir = std::env::temp_dir().join(format!("migrate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let layout = crate::workspace::layout::Layout::at(&dir);
        crate::workspace::layout::ensure_layout(&layout, "m").unwrap();

        let legacy_file = dir.join("legacy.db");
        let legacy = Connection::open(&legacy_file).unwrap();
        legacy.execute_batch("PRAGMA foreign_keys = ON").unwrap();
        schema::run_migrations(&legacy).unwrap();
        legacy
            .execute(
                "INSERT INTO stickers (id, title, content) VALUES (7, '老题目', '# 老内容')",
                [],
            )
            .unwrap();
        legacy
            .execute(
                "INSERT INTO todo_blocks (id, sticker_id, title) VALUES ('t-1', 7, '任务')",
                [],
            )
            .unwrap();
        legacy.close().unwrap();

        let new = Connection::open_in_memory().unwrap();
        new.execute_batch("PRAGMA foreign_keys = ON").unwrap();
        schema::run_migrations(&new).unwrap();

        let summary = run(&legacy_file, &new, &dir).unwrap();
        assert_eq!(summary.stickers, 1);
        assert_eq!(summary.todo_blocks, 1);

        let md = crate::workspace::md_store::load(&dir, "7-老题目.md")
            .unwrap()
            .unwrap();
        assert_eq!(md, "# 老内容");

        let row: String = new
            .query_row("SELECT content FROM stickers WHERE id=7", [], |r| r.get(0))
            .unwrap();
        assert_eq!(row, "");
        let todo: String = new
            .query_row("SELECT title FROM todo_blocks WHERE id='t-1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(todo, "任务");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 事务中途失败（悬空 FK）→ 全部 DB 插入回滚，不留半迁移状态。
    #[test]
    fn run_rolls_back_all_db_inserts_on_mid_phase_error() {
        let dir = std::env::temp_dir().join(format!("migrate-rb-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let layout = crate::workspace::layout::Layout::at(&dir);
        crate::workspace::layout::ensure_layout(&layout, "rb").unwrap();

        // 旧库：不启用 FK（SCHEMA_SQL 自带 PRAGMA foreign_keys=ON，须显式关掉）。
        let legacy_file = dir.join("legacy-rb.db");
        let legacy = Connection::open(&legacy_file).unwrap();
        schema::run_migrations(&legacy).unwrap();
        legacy.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
        legacy
            .execute(
                "INSERT INTO stickers (id, title, content) VALUES (7, '正常', '# ok')",
                [],
            )
            .unwrap();
        // 合法条目 rowid=1 先于悬空条目 rowid=2 → ORDER BY rowid 保证中途才失败。
        legacy
            .execute(
                "INSERT INTO todo_items (sticker_id, text) VALUES (7, '合法条目')",
                [],
            )
            .unwrap();
        legacy
            .execute(
                "INSERT INTO todo_items (sticker_id, text) VALUES (777, '悬空条目')",
                [],
            )
            .unwrap();
        legacy.close().unwrap();

        // 新库：与外键约束（真实连接同配置）。
        let new = Connection::open_in_memory().unwrap();
        new.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        schema::run_migrations(&new).unwrap();

        let result = run(&legacy_file, &new, &dir);
        assert!(result.is_err(), "悬空 FK 应导致迁移失败");

        let stickers: i64 = new
            .query_row("SELECT COUNT(*) FROM stickers", [], |r| r.get(0))
            .unwrap();
        let todos: i64 = new
            .query_row("SELECT COUNT(*) FROM todo_items", [], |r| r.get(0))
            .unwrap();
        assert_eq!(stickers, 0, "事务回滚：stickers 不应残留");
        assert_eq!(todos, 0, "事务回滚：todo_items 不应残留");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
