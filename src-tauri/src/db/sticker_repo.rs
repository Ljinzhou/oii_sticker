//! `sticker_repo` 提供对 `stickers` 与 `sticker_attrs` 表的同步 CRUD。
//!
//! 所有方法以 `&Connection` 为入参，由调用方（state.rs / commands.rs）
//! 负责把 DB IO 派发到 `spawn_blocking` 上运行，避免阻塞 UI 线程。

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::models::{Sticker, StickerAttrs};

/// 新建一条便签，返回自增 id。
pub fn insert(conn: &Connection, s: &NewSticker) -> Result<i64> {
    let bg = s.bg_color.as_deref();
    conn.execute(
        "INSERT INTO stickers
           (parent_id, title, content, heading_level,
            pos_x, pos_y, width, height, opacity, bg_color,
            always_on_top, auto_scroll, is_completed, display_mode)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                 ?11, ?12, 0, 'display')",
        params![
            s.parent_id,
            s.title,
            s.content,
            s.heading_level,
            s.pos_x,
            s.pos_y,
            s.width,
            s.height,
            s.opacity,
            bg,
            s.always_on_top as i32,
            s.auto_scroll as i32,
        ],
    )
    .context("插入便签失败")?;
    Ok(conn.last_insert_rowid())
}

/// 按 id 读取一条便签。
pub fn get(conn: &Connection, id: i64) -> Result<Option<Sticker>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, parent_id, title, content, heading_level,
                pos_x, pos_y, width, height, opacity, bg_color,
                always_on_top, auto_scroll, is_completed, alert_active,
                display_mode, created_at, updated_at
           FROM stickers WHERE id = ?1",
    )?;
    let s = stmt.query_row(params![id], row_to_sticker).optional()?;
    Ok(s)
}

/// 列出全部便签。
pub fn list_all(conn: &Connection) -> Result<Vec<Sticker>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, parent_id, title, content, heading_level,
                pos_x, pos_y, width, height, opacity, bg_color,
                always_on_top, auto_scroll, is_completed, alert_active,
                display_mode, created_at, updated_at
           FROM stickers ORDER BY id",
    )?;
    let rows = stmt
        .query_map([], row_to_sticker)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// 部分更新便签字段；只覆盖传入的 Option 字段。
pub fn update(conn: &Connection, id: i64, patch: &StickerPatch) -> Result<()> {
    conn.execute(
        "UPDATE stickers SET
            title        = COALESCE(?2, title),
            content      = COALESCE(?3, content),
            pos_x        = COALESCE(?4, pos_x),
            pos_y        = COALESCE(?5, pos_y),
            width        = COALESCE(?6, width),
            height       = COALESCE(?7, height),
            opacity      = COALESCE(?8, opacity),
            bg_color     = COALESCE(?9, bg_color),
            always_on_top= COALESCE(?10, always_on_top),
            auto_scroll  = COALESCE(?11, auto_scroll),
            is_completed = COALESCE(?12, is_completed),
            alert_active = COALESCE(?13, alert_active),
            display_mode = COALESCE(?14, display_mode),
            updated_at   = datetime('now')
         WHERE id = ?1",
        params![
            id,
            patch.title,
            patch.content,
            patch.pos_x,
            patch.pos_y,
            patch.width,
            patch.height,
            patch.opacity,
            patch.bg_color,
            patch.always_on_top.map(|b| b as i32),
            patch.auto_scroll.map(|b| b as i32),
            patch.is_completed.map(|b| b as i32),
            patch.alert_active.map(|b| b as i32),
            patch.display_mode,
        ],
    )
    .context("更新便签失败")?;
    Ok(())
}

/// 删除一条便签，依赖外键 ON DELETE CASCADE 清理子树与关联记录。
pub fn delete(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM stickers WHERE id = ?1", params![id])
        .context("删除便签失败")?;
    Ok(())
}

/// 写入或覆盖 sticker_attrs（提醒规则）。
pub fn upsert_attrs(conn: &Connection, a: &StickerAttrs) -> Result<()> {
    conn.execute(
        "INSERT INTO sticker_attrs
            (sticker_id, due_date, remind_at, remind_rule, is_recurring)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(sticker_id) DO UPDATE SET
            due_date     = excluded.due_date,
            remind_at    = excluded.remind_at,
            remind_rule  = excluded.remind_rule,
            is_recurring = excluded.is_recurring",
        params![
            a.sticker_id,
            a.due_date,
            a.remind_at,
            a.remind_rule,
            a.is_recurring as i32,
        ],
    )
    .context("写入 sticker_attrs 失败")?;
    Ok(())
}

/// 读取 sticker_attrs，没有则返回 None。
pub fn get_attrs(conn: &Connection, id: i64) -> Result<Option<StickerAttrs>> {
    let mut stmt = conn.prepare_cached(
        "SELECT sticker_id, due_date, remind_at, remind_rule, is_recurring
           FROM sticker_attrs WHERE sticker_id = ?1",
    )?;
    let r = stmt
        .query_row(params![id], row_to_attrs)
        .optional()?;
    Ok(r)
}

/// 列出所有提醒时间非空且未完成的 sticker_attrs（给 scheduler 用）。
pub fn list_pending_reminders(conn: &Connection) -> Result<Vec<StickerAttrs>> {
    let mut stmt = conn.prepare_cached(
        "SELECT sticker_id, due_date, remind_at, remind_rule, is_recurring
           FROM sticker_attrs
          WHERE remind_at IS NOT NULL
            AND (is_recurring = 1 OR remind_at > datetime('now'))",
    )?;
    let rows = stmt
        .query_map([], row_to_attrs)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn row_to_sticker(row: &rusqlite::Row<'_>) -> rusqlite::Result<Sticker> {
    Ok(Sticker {
        id: row.get(0)?,
        parent_id: row.get(1)?,
        title: row.get(2)?,
        content: row.get(3)?,
        heading_level: row.get(4)?,
        pos_x: row.get(5)?,
        pos_y: row.get(6)?,
        width: row.get(7)?,
        height: row.get(8)?,
        opacity: row.get(9)?,
        bg_color: row.get(10)?,
        always_on_top: row.get(11)?,
        auto_scroll: row.get(12)?,
        is_completed: row.get(13)?,
        alert_active: row.get(14)?,
        display_mode: row.get(15)?,
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
    })
}

fn row_to_attrs(row: &rusqlite::Row<'_>) -> rusqlite::Result<StickerAttrs> {
    Ok(StickerAttrs {
        sticker_id: row.get(0)?,
        due_date: row.get(1)?,
        remind_at: row.get(2)?,
        remind_rule: row.get(3)?,
        is_recurring: row.get(4)?,
    })
}

/// 新建便签入参。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NewSticker {
    pub parent_id: Option<i64>,
    pub title: String,
    pub content: String,
    pub heading_level: i32,
    pub pos_x: i32,
    pub pos_y: i32,
    pub width: i32,
    pub height: i32,
    pub opacity: f32,
    pub bg_color: Option<String>,
    pub always_on_top: bool,
    pub auto_scroll: bool,
}

/// 部分更新便签入参：None 表示不动该字段。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StickerPatch {
    pub title: Option<String>,
    pub content: Option<String>,
    pub pos_x: Option<i32>,
    pub pos_y: Option<i32>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub opacity: Option<f32>,
    pub bg_color: Option<String>,
    pub always_on_top: Option<bool>,
    pub auto_scroll: Option<bool>,
    pub is_completed: Option<bool>,
    pub alert_active: Option<bool>,
    pub display_mode: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::connection::open;

    fn test_conn() -> Connection {
        // 用临时文件库测试（WAL 模式在内存库上可用但行为略有差异）
        let dir = std::env::temp_dir().join(format!("oii-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let conn = open(&dir.join("test.db")).unwrap();
        crate::db::schema::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn sticker_crud_roundtrip() {
        let conn = test_conn();
        let new = NewSticker {
            title: "测试便签".into(),
            content: "# 标题\n正文".into(),
            pos_x: 10,
            pos_y: 20,
            width: 300,
            height: 400,
            opacity: 0.85,
            bg_color: Some("#FFEEAA".into()),
            always_on_top: true,
            ..Default::default()
        };
        let id = insert(&conn, &new).unwrap();
        assert!(id > 0);

        let s = get(&conn, id).unwrap().expect("应能读到");
        assert_eq!(s.title, "测试便签");
        assert_eq!(s.display_mode, "display");
        assert!(s.always_on_top);
        assert_eq!(s.mode(), crate::models::StickerMode::Display);

        // 部分更新
        update(
            &conn,
            id,
            &StickerPatch {
                title: Some("改名".into()),
                display_mode: Some("interact".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let s = get(&conn, id).unwrap().unwrap();
        assert_eq!(s.title, "改名");
        assert_eq!(s.mode(), crate::models::StickerMode::Interact);
        assert_eq!(s.content, "# 标题\n正文"); // 未更新的字段保留

        // 列表
        let all = list_all(&conn).unwrap();
        assert_eq!(all.len(), 1);

        // attrs 写入与读取
        let attrs = StickerAttrs {
            sticker_id: id,
            due_date: None,
            remind_at: Some("2026-12-31 10:00:00".into()),
            remind_rule: Some("weekly".into()),
            is_recurring: true,
        };
        upsert_attrs(&conn, &attrs).unwrap();
        let a = get_attrs(&conn, id).unwrap().unwrap();
        assert_eq!(a.remind_at.as_deref(), Some("2026-12-31 10:00:00"));
        assert!(a.is_recurring);

        // pending reminders 查询
        let pending = list_pending_reminders(&conn).unwrap();
        assert_eq!(pending.len(), 1);

        // 删除后 attrs 级联清理
        delete(&conn, id).unwrap();
        assert!(get(&conn, id).unwrap().is_none());
        assert!(get_attrs(&conn, id).unwrap().is_none());
    }
}
