//! `todo_repo` 提供对 `todo_items` 表的 CRUD。
//! 相比源项目补齐了提醒字段（due_date/remind_at/remind_rule/is_recurring）
//! 的独立写入函数，供"给单个 todo 设提醒"使用。

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

use crate::models::TodoItem;

/// 插入一条 todo，返回新 id。
pub fn insert(
    conn: &Connection,
    sticker_id: i64,
    text: &str,
    sort_order: i32,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO todo_items (sticker_id, text, sort_order)
         VALUES (?1, ?2, ?3)",
        params![sticker_id, text, sort_order],
    )
    .context("插入 todo 失败")?;
    Ok(conn.last_insert_rowid())
}

/// 读取单条 todo。
pub fn get(conn: &Connection, id: i64) -> Result<Option<TodoItem>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, sticker_id, child_sticker_id, text, done, completed_at,
                sort_order, due_date, remind_at, remind_rule, is_recurring
           FROM todo_items WHERE id = ?1",
    )?;
    let r = stmt.query_row(params![id], row_to_todo).optional()?;
    Ok(r)
}

/// 列出某个便签的所有 todo，按 sort_order 排序。
pub fn list_by_sticker(conn: &Connection, sticker_id: i64) -> Result<Vec<TodoItem>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, sticker_id, child_sticker_id, text, done, completed_at,
                sort_order, due_date, remind_at, remind_rule, is_recurring
           FROM todo_items WHERE sticker_id = ?1 ORDER BY sort_order, id",
    )?;
    let rows = stmt
        .query_map(params![sticker_id], row_to_todo)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// 切换完成状态。
pub fn set_done(conn: &Connection, id: i64, done: bool) -> Result<()> {
    conn.execute(
        "UPDATE todo_items SET
            done = ?2,
            completed_at = CASE WHEN ?2 = 1 THEN datetime('now') ELSE NULL END
         WHERE id = ?1",
        params![id, done as i32],
    )
    .context("切换 todo 状态失败")?;
    Ok(())
}

/// 写入/覆盖 todo 的提醒字段（源项目缺失，本工程补齐）。
pub fn set_reminder_fields(
    conn: &Connection,
    id: i64,
    due_date: Option<&str>,
    remind_at: Option<&str>,
    remind_rule: Option<&str>,
    is_recurring: bool,
) -> Result<()> {
    conn.execute(
        "UPDATE todo_items SET
            due_date     = ?2,
            remind_at    = ?3,
            remind_rule  = ?4,
            is_recurring = ?5
         WHERE id = ?1",
        params![id, due_date, remind_at, remind_rule, is_recurring as i32],
    )
    .context("写入 todo 提醒字段失败")?;
    Ok(())
}

/// 清空某个便签下的所有 todo（heading_sync 重新生成前调用）。
pub fn delete_by_sticker(conn: &Connection, sticker_id: i64) -> Result<()> {
    conn.execute(
        "DELETE FROM todo_items WHERE sticker_id = ?1",
        params![sticker_id],
    )
    .context("按便签删除 todo 失败")?;
    Ok(())
}

fn row_to_todo(row: &rusqlite::Row<'_>) -> rusqlite::Result<TodoItem> {
    Ok(TodoItem {
        id: row.get(0)?,
        sticker_id: row.get(1)?,
        child_sticker_id: row.get(2)?,
        text: row.get(3)?,
        done: row.get(4)?,
        completed_at: row.get(5)?,
        sort_order: row.get(6)?,
        due_date: row.get(7)?,
        remind_at: row.get(8)?,
        remind_rule: row.get(9)?,
        is_recurring: row.get(10)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{schema, sticker_repo};

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        schema::run_migrations(&conn).unwrap();
        conn
    }

    fn make_sticker(conn: &Connection) -> i64 {
        sticker_repo::insert(
            conn,
            &sticker_repo::NewSticker {
                title: "todo 宿主".into(),
                ..Default::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn todo_crud_and_reminder_fields() {
        let conn = test_conn();
        let sid = make_sticker(&conn);

        let t1 = insert(&conn, sid, "任务 A", 0).unwrap();
        let t2 = insert(&conn, sid, "任务 B", 1).unwrap();

        let list = list_by_sticker(&conn, sid).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].text, "任务 A");

        set_done(&conn, t1, true).unwrap();
        let done = get(&conn, t1).unwrap().unwrap();
        assert!(done.done);
        assert!(done.completed_at.is_some());

        // 提醒字段写入（源项目缺失的能力）
        set_reminder_fields(
            &conn,
            t2,
            Some("2026-12-01"),
            Some("2026-12-01 09:00:00"),
            Some("daily"),
            true,
        )
        .unwrap();
        let r = get(&conn, t2).unwrap().unwrap();
        assert_eq!(r.remind_at.as_deref(), Some("2026-12-01 09:00:00"));
        assert_eq!(r.remind_rule.as_deref(), Some("daily"));
        assert!(r.is_recurring);

        // 清空
        delete_by_sticker(&conn, sid).unwrap();
        assert!(list_by_sticker(&conn, sid).unwrap().is_empty());
    }
}
