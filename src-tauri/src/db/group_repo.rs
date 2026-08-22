//! 便签分组 CRUD（类文件夹管理）。

use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

use crate::models::StickerGroup;

fn row_to_group(row: &rusqlite::Row<'_>) -> rusqlite::Result<StickerGroup> {
    Ok(StickerGroup {
        id: row.get(0)?,
        name: row.get(1)?,
        sort_order: row.get(2)?,
        created_at: row.get(3)?,
    })
}

pub fn list(conn: &Connection) -> Result<Vec<StickerGroup>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, name, sort_order, created_at FROM sticker_groups ORDER BY sort_order, id",
    )?;
    let rows = stmt.query_map([], row_to_group)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn get(conn: &Connection, id: i64) -> Result<Option<StickerGroup>> {
    conn.query_row(
        "SELECT id, name, sort_order, created_at FROM sticker_groups WHERE id = ?1",
        params![id],
        row_to_group,
    )
    .optional()
    .map_err(Into::into)
}

pub fn create(conn: &Connection, name: &str) -> Result<StickerGroup> {
    let name = name.trim();
    if name.is_empty() {
        bail!("分组名称不能为空");
    }
    conn.execute("INSERT INTO sticker_groups (name) VALUES (?1)", params![name])
        .context("创建分组失败")?;
    get(conn, conn.last_insert_rowid())?.context("创建分组后读取失败")
}

pub fn rename(conn: &Connection, id: i64, name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        bail!("分组名称不能为空");
    }
    conn.execute("UPDATE sticker_groups SET name = ?2 WHERE id = ?1", params![id, name])
        .context("重命名分组失败")?;
    Ok(())
}

/// 删除分组。mode：
/// - "to-default"：组内便签由外键 ON DELETE SET NULL 自动回默认组；
/// - "with-stickers"：删除组内全部便签并返回其 id（调用方负责 md/assets 清理）。
///
/// 全程单事务：任一步失败整体回滚，避免"便签已删、组还在"的中间态。
pub fn delete(conn: &Connection, id: i64, mode: &str) -> Result<Vec<i64>> {
    if mode != "to-default" && mode != "with-stickers" {
        bail!("未知的删除模式：{mode}");
    }
    get(conn, id)?.context("分组不存在")?;

    let tx = conn
        .unchecked_transaction()
        .context("开启删除分组事务失败")?;
    let mut removed = Vec::new();
    if mode == "with-stickers" {
        {
            let mut stmt = tx.prepare("SELECT id FROM stickers WHERE group_id = ?1")?;
            removed = stmt
                .query_map(params![id], |r| r.get(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
        }
        tx.execute(
            "DELETE FROM stickers WHERE group_id = ?1",
            params![id],
        )
        .context("删除分组内便签失败")?;
    }
    tx.execute("DELETE FROM sticker_groups WHERE id = ?1", params![id])
        .context("删除分组失败")?;
    tx.commit().context("提交删除分组事务失败")?;
    Ok(removed)
}

/// 移动便签到指定分组；None = 回默认组。目标分组必须存在。
pub fn move_sticker(conn: &Connection, sticker_id: i64, group_id: Option<i64>) -> Result<()> {
    if let Some(gid) = group_id {
        get(conn, gid)?.context("目标分组不存在")?;
    }
    conn.execute(
        "UPDATE stickers SET group_id = ?2, updated_at = datetime('now') WHERE id = ?1",
        params![sticker_id, group_id],
    )
    .context("移动便签分组失败")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{schema, sticker_repo};

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("PRAGMA foreign_keys = ON").unwrap();
        schema::run_migrations(&c).unwrap();
        c
    }

    fn make_sticker(c: &Connection) -> i64 {
        sticker_repo::insert(c, &sticker_repo::NewSticker { title: "s".into(), ..Default::default() }).unwrap()
    }

    #[test]
    fn create_list_rename_roundtrip() {
        let c = conn();
        let g = create(&c, "工作").unwrap();
        assert_eq!(g.name, "工作");
        assert!(create(&c, "  ").is_err(), "空名拒绝");
        rename(&c, g.id, "学习").unwrap();
        assert_eq!(get(&c, g.id).unwrap().unwrap().name, "学习");
        assert!(list(&c).unwrap().iter().any(|x| x.id == g.id));
    }

    #[test]
    fn delete_to_default_sets_null_and_with_stickers_removes() {
        let c = conn();
        let g = create(&c, "临时").unwrap();
        let s1 = make_sticker(&c);
        c.execute("UPDATE stickers SET group_id=?2 WHERE id=?1", params![s1, g.id]).unwrap();

        // to-default：FK SET NULL 自动回默认组
        delete(&c, g.id, "to-default").unwrap();
        let gid: Option<i64> = c.query_row("SELECT group_id FROM stickers WHERE id=?1", params![s1], |r| r.get(0)).unwrap();
        assert_eq!(gid, None);
        assert!(get(&c, g.id).unwrap().is_none());

        // with-stickers：返回并删除组内便签
        let g2 = create(&c, "危险").unwrap();
        let s2 = make_sticker(&c);
        c.execute("UPDATE stickers SET group_id=?2 WHERE id=?1", params![s2, g2.id]).unwrap();
        let removed = delete(&c, g2.id, "with-stickers").unwrap();
        assert_eq!(removed, vec![s2]);
        assert!(get(&c, g2.id).unwrap().is_none());
        let n: i64 = c.query_row("SELECT COUNT(*) FROM stickers", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1, "s1 还在（属于默认组）");
    }

    #[test]
    fn move_sticker_validates_target() {
        let c = conn();
        let s = make_sticker(&c);
        let g = create(&c, "G").unwrap();
        move_sticker(&c, s, Some(g.id)).unwrap();
        let gid: i64 = c.query_row("SELECT group_id FROM stickers WHERE id=?1", params![s], |r| r.get(0)).unwrap();
        assert_eq!(gid, g.id);
        move_sticker(&c, s, None).unwrap(); // 移出回默认组
        assert!(move_sticker(&c, s, Some(99999)).is_err(), "目标分组不存在应拒绝");
    }

    #[test]
    fn delete_rejects_unknown_mode_and_keeps_group() {
        let c = conn();
        let g = create(&c, "G").unwrap();
        assert!(delete(&c, g.id, "explode").is_err(), "未知模式应拒绝");
        assert!(get(&c, g.id).unwrap().is_some(), "未知模式不得删组");
    }
}
