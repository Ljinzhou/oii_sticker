//! 独立 Todo 块的 CRUD，保留 todo_items 作为旧 Markdown 任务实现。

use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::models::{TodoBlock, TodoPatch};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

fn next_id() -> String {
    let millis = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
    let sequence = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    format!("t-{millis:x}-{sequence:x}")
}

pub fn create(conn: &Connection, sticker_id: i64, parent_id: Option<&str>) -> Result<TodoBlock> {
    if let Some(parent_id) = parent_id {
        let parent = get(conn, parent_id)?.context("父任务不存在")?;
        if parent.sticker_id != sticker_id || parent.parent_id.is_some() {
            bail!("子任务必须属于同一便签且只能嵌套一层");
        }
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM todo_blocks WHERE parent_id = ?1)",
            params![parent_id], |r| r.get(0),
        )?;
        if exists { bail!("一个父任务最多只能有一个子任务"); }
    }
    let id = next_id();
    conn.execute(
        "INSERT INTO todo_blocks (id, sticker_id, parent_id) VALUES (?1, ?2, ?3)",
        params![id, sticker_id, parent_id],
    ).context("创建 Todo 块失败")?;
    get(conn, &id)?.context("创建 Todo 块后读取失败")
}

pub fn get(conn: &Connection, id: &str) -> Result<Option<TodoBlock>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, sticker_id, title, description, is_completed, parent_id, reminder_at, due_at, repeat_rule, created_at, updated_at
         FROM todo_blocks WHERE id = ?1",
    )?;
    Ok(stmt.query_row(params![id], row_to_block).optional()?)
}

pub fn list_by_sticker(conn: &Connection, sticker_id: i64) -> Result<Vec<TodoBlock>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, sticker_id, title, description, is_completed, parent_id, reminder_at, due_at, repeat_rule, created_at, updated_at
         FROM todo_blocks WHERE sticker_id = ?1
         ORDER BY parent_id IS NOT NULL, created_at, id",
    )?;
    let rows = stmt
        .query_map(params![sticker_id], row_to_block)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn update(conn: &Connection, id: &str, patch: &TodoPatch) -> Result<TodoBlock> {
    let current = get(conn, id)?.context("Todo 块不存在")?;
    let child = current.parent_id.is_some();
    conn.execute(
        "UPDATE todo_blocks SET
            title = COALESCE(?2, title), description = COALESCE(?3, description),
            is_completed = COALESCE(?4, is_completed),
            reminder_at = CASE WHEN ?5 OR ?6 IS NULL THEN reminder_at ELSE NULLIF(?6, '') END,
            due_at = CASE WHEN ?7 OR ?8 IS NULL THEN due_at ELSE NULLIF(?8, '') END,
            repeat_rule = CASE WHEN ?9 OR ?10 IS NULL THEN repeat_rule ELSE NULLIF(?10, '') END,
            updated_at = datetime('now') WHERE id = ?1",
        params![
            id, patch.title, patch.description, patch.is_completed.map(|v| v as i32),
            child, patch.reminder_at, child, patch.due_at, child, patch.repeat_rule,
        ],
    ).context("更新 Todo 块失败")?;
    get(conn, id)?.context("更新 Todo 块后读取失败")
}

pub fn delete(conn: &Connection, id: &str) -> Result<Option<i64>> {
    let sticker_id = get(conn, id)?.map(|item| item.sticker_id);
    if sticker_id.is_some() { conn.execute("DELETE FROM todo_blocks WHERE id = ?1", params![id]).context("删除 Todo 块失败")?; }
    Ok(sticker_id)
}

fn row_to_block(row: &rusqlite::Row<'_>) -> rusqlite::Result<TodoBlock> {
    Ok(TodoBlock {
        id: row.get(0)?, sticker_id: row.get(1)?, title: row.get(2)?, description: row.get(3)?,
        is_completed: row.get(4)?, parent_id: row.get(5)?, reminder_at: row.get(6)?, due_at: row.get(7)?,
        repeat_rule: row.get(8)?, created_at: row.get(9)?, updated_at: row.get(10)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{schema, sticker_repo};

    fn conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON").unwrap();
        schema::run_migrations(&conn).unwrap();
        conn
    }

    fn sticker(conn: &Connection) -> i64 {
        sticker_repo::insert(conn, &sticker_repo::NewSticker { title: "测试".into(), ..Default::default() }).unwrap()
    }

    #[test]
    fn creates_parent_and_only_one_direct_child() {
        let conn = conn();
        let sticker_id = sticker(&conn);
        let parent = create(&conn, sticker_id, None).unwrap();
        let child = create(&conn, sticker_id, Some(&parent.id)).unwrap();
        assert_eq!(child.parent_id.as_deref(), Some(parent.id.as_str()));
        assert!(create(&conn, sticker_id, Some(&parent.id)).is_err());
        assert!(create(&conn, sticker_id, Some(&child.id)).is_err());
    }

    #[test]
    fn child_cannot_write_advanced_fields_and_delete_cascades() {
        let conn = conn();
        let sticker_id = sticker(&conn);
        let parent = create(&conn, sticker_id, None).unwrap();
        let child = create(&conn, sticker_id, Some(&parent.id)).unwrap();
        let updated = update(&conn, &child.id, &TodoPatch { reminder_at: Some("2030-01-01T00:00:00Z".into()), ..Default::default() }).unwrap();
        assert!(updated.reminder_at.is_none());
        delete(&conn, &parent.id).unwrap();
        assert!(get(&conn, &child.id).unwrap().is_none());
    }
}
