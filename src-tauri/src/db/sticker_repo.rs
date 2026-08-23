//! `sticker_repo` 提供对 `stickers` 表的同步 CRUD。
//!
//! 所有方法以 `&Connection` 为入参，由调用方（state.rs / commands.rs）
//! 负责把 DB IO 派发到 `spawn_blocking` 上运行，避免阻塞 UI 线程。

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::models::Sticker;

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
                always_on_top, auto_scroll, is_completed,
                group_id, display_mode, created_at, updated_at,
                window_hidden
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
                always_on_top, auto_scroll, is_completed,
                group_id, display_mode, created_at, updated_at,
                window_hidden
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
            display_mode = COALESCE(?13, display_mode),
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
            patch.display_mode,
        ],
    )
    .context("更新便签失败")?;
    Ok(())
}

/// 记录窗口隐藏状态（程序退出对隐藏窗口、隐藏命令调用）。
/// 只改 window_hidden，保留最近一次几何，便于重新显示时恢复位置。
pub fn update_window_hidden(conn: &Connection, id: i64, hidden: bool) -> Result<()> {
    conn.execute(
        "UPDATE stickers SET window_hidden = ?2, updated_at = datetime('now')
         WHERE id = ?1",
        params![id, hidden as i32],
    )
    .context("更新窗口隐藏状态失败")?;
    Ok(())
}

/// 记录窗口位置与尺寸（程序退出对显示窗口调用），并标记为显示态。
pub fn update_window_bounds(conn: &Connection, id: i64, x: i32, y: i32, w: i32, h: i32) -> Result<()> {
    conn.execute(
        "UPDATE stickers SET
            pos_x = ?2, pos_y = ?3, width = ?4, height = ?5,
            window_hidden = 0,
            updated_at = datetime('now')
         WHERE id = ?1",
        params![id, x, y, w, h],
    )
    .context("更新窗口几何失败")?;
    Ok(())
}

/// 删除一条便签，依赖外键 ON DELETE CASCADE 清理子树与关联记录。
pub fn delete(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM stickers WHERE id = ?1", params![id])
        .context("删除便签失败")?;
    Ok(())
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
        group_id: row.get(14)?,
        display_mode: row.get(15)?,
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
        window_hidden: row.get(18)?,
    })
}

/// 新建便签入参。
/// `#[serde(default)]`：前端未传的字段（如 heading_level）用 Default，
/// 避免 invoke 报 "missing field"。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
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
    pub display_mode: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::connection::open;

    /// 前端可能不传可选字段（如 heading_level）：serde(default) 保证不报 missing field。
    #[test]
    fn new_sticker_missing_fields_deserializes() {
        let json = r##"{
            "title": "测试",
            "content": "正文",
            "pos_x": 10,
            "pos_y": 20,
            "width": 300,
            "height": 400,
            "opacity": 0.9,
            "bg_color": "#FFEEAA",
            "always_on_top": false,
            "auto_scroll": false
        }"##;
        let s: NewSticker = serde_json::from_str(json).expect("缺失 heading_level 等字段也应成功");
        assert_eq!(s.title, "测试");
        assert_eq!(s.heading_level, 0);
        assert_eq!(s.parent_id, None);
        assert_eq!(s.bg_color.as_deref(), Some("#FFEEAA"));
    }

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
            content: "# 标题
正文".into(),
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
        assert!(!s.window_hidden, "新便签默认显示");

        // 窗口状态：隐藏标记 + 几何记录
        update_window_hidden(&conn, id, true).unwrap();
        let s = get(&conn, id).unwrap().unwrap();
        assert!(s.window_hidden);
        assert_eq!((s.pos_x, s.pos_y, s.width, s.height), (10, 20, 300, 400));
        update_window_bounds(&conn, id, 88, 66, 320, 240).unwrap();
        let s = get(&conn, id).unwrap().unwrap();
        assert!(!s.window_hidden, "记录几何时视为显示");
        assert_eq!((s.pos_x, s.pos_y, s.width, s.height), (88, 66, 320, 240));

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
        assert_eq!(s.content, "# 标题
正文"); // 未更新的字段保留

        // 列表
        let all = list_all(&conn).unwrap();
        assert_eq!(all.len(), 1);

        // 删除级联清理
        delete(&conn, id).unwrap();
        assert!(get(&conn, id).unwrap().is_none());
    }
}
