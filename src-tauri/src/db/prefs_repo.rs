//! `prefs_repo` 提供对 `sticker_prefs` 表的同步 CRUD。
//!
//! 每条便签最多对应一行 prefs 记录；缺失行视为"沿用 system_config 中的
//! 默认便签偏好"。

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

use crate::models::StickerPrefs;

/// 读取便签 prefs；没有则返回 None。
pub fn get(conn: &Connection, sticker_id: i64) -> Result<Option<StickerPrefs>> {
    let mut stmt = conn.prepare_cached(
        "SELECT sticker_id, opacity, title_centered, title_font_size,
                body_font_size, bg_color, text_color, auto_scroll_speed
           FROM sticker_prefs WHERE sticker_id = ?1",
    )?;
    let r = stmt
        .query_row(params![sticker_id], row_to_prefs)
        .optional()?;
    Ok(r)
}

/// 写入或覆盖便签 prefs。
pub fn upsert(conn: &Connection, p: &StickerPrefs) -> Result<()> {
    conn.execute(
        "INSERT INTO sticker_prefs
            (sticker_id, opacity, title_centered, title_font_size,
             body_font_size, bg_color, text_color, auto_scroll_speed)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(sticker_id) DO UPDATE SET
            opacity           = excluded.opacity,
            title_centered    = excluded.title_centered,
            title_font_size   = excluded.title_font_size,
            body_font_size    = excluded.body_font_size,
            bg_color          = excluded.bg_color,
            text_color        = excluded.text_color,
            auto_scroll_speed = excluded.auto_scroll_speed",
        params![
            p.sticker_id,
            p.opacity,
            p.title_centered.map(|b| b as i32),
            p.title_font_size,
            p.body_font_size,
            p.bg_color,
            p.text_color,
            p.auto_scroll_speed,
        ],
    )
    .context("写入 sticker_prefs 失败")?;
    Ok(())
}

/// 删除便签 prefs（依赖外键 ON DELETE CASCADE 自动级联，备用）。
pub fn delete(conn: &Connection, sticker_id: i64) -> Result<()> {
    conn.execute(
        "DELETE FROM sticker_prefs WHERE sticker_id = ?1",
        params![sticker_id],
    )
    .context("删除 sticker_prefs 失败")?;
    Ok(())
}

/// 列出全部 prefs（启动时预热用）。
pub fn list_all(conn: &Connection) -> Result<Vec<StickerPrefs>> {
    let mut stmt = conn.prepare_cached(
        "SELECT sticker_id, opacity, title_centered, title_font_size,
                body_font_size, bg_color, text_color, auto_scroll_speed
           FROM sticker_prefs",
    )?;
    let rows = stmt
        .query_map([], row_to_prefs)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn row_to_prefs(row: &rusqlite::Row<'_>) -> rusqlite::Result<StickerPrefs> {
    Ok(StickerPrefs {
        sticker_id: row.get(0)?,
        opacity: row.get(1)?,
        title_centered: row.get(2)?,
        title_font_size: row.get(3)?,
        body_font_size: row.get(4)?,
        bg_color: row.get(5)?,
        text_color: row.get(6)?,
        auto_scroll_speed: row.get(7)?,
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
                title: "prefs 宿主".into(),
                ..Default::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn prefs_upsert_get_delete() {
        let conn = test_conn();
        let sid = make_sticker(&conn);
        // 没有则 None
        assert!(get(&conn, sid).unwrap().is_none());

        let p = StickerPrefs {
            sticker_id: sid,
            opacity: Some(0.7),
            title_centered: Some(true),
            title_font_size: Some(16),
            body_font_size: None,
            bg_color: Some("#112233".into()),
            text_color: None,
            auto_scroll_speed: Some(40),
        };
        upsert(&conn, &p).unwrap();

        let got = get(&conn, sid).unwrap().unwrap();
        assert_eq!(got.opacity, Some(0.7));
        assert_eq!(got.title_centered, Some(true));
        assert_eq!(got.title_font_size, Some(16));
        assert_eq!(got.body_font_size, None);
        assert_eq!(got.bg_color.as_deref(), Some("#112233"));
        assert_eq!(got.auto_scroll_speed, Some(40));

        // 覆盖：只更新部分字段会整体覆盖该行
        let p2 = StickerPrefs {
            sticker_id: sid,
            opacity: Some(0.5),
            ..Default::default()
        };
        upsert(&conn, &p2).unwrap();
        let got = get(&conn, sid).unwrap().unwrap();
        assert_eq!(got.opacity, Some(0.5));
        assert_eq!(got.bg_color, None); // 已被覆盖为 None

        assert_eq!(list_all(&conn).unwrap().len(), 1);
        delete(&conn, sid).unwrap();
        assert!(get(&conn, sid).unwrap().is_none());
    }
}
