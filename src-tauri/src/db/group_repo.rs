//! 便签分组 CRUD（类文件夹管理：支持无限层级与颜色）。

use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

use crate::models::StickerGroup;

const COLS: &str = "id, name, sort_order, created_at, parent_id, color";

fn row_to_group(row: &rusqlite::Row<'_>) -> rusqlite::Result<StickerGroup> {
    Ok(StickerGroup {
        id: row.get(0)?,
        name: row.get(1)?,
        sort_order: row.get(2)?,
        created_at: row.get(3)?,
        parent_id: row.get(4)?,
        color: row.get(5)?,
    })
}

/// 全部分组（同级按 sort_order 排序；层级由 parent_id 表达，前端自行组树）。
pub fn list(conn: &Connection) -> Result<Vec<StickerGroup>> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT {COLS} FROM sticker_groups ORDER BY sort_order, id"
    ))?;
    let rows = stmt.query_map([], row_to_group)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn get(conn: &Connection, id: i64) -> Result<Option<StickerGroup>> {
    conn.query_row(
        &format!("SELECT {COLS} FROM sticker_groups WHERE id = ?1"),
        params![id],
        row_to_group,
    )
    .optional()
    .map_err(Into::into)
}

/// 新建分组；`parent_id` 为 None 表示顶层。父分组必须存在，新组排在同级末尾。
pub fn create(conn: &Connection, name: &str, parent_id: Option<i64>) -> Result<StickerGroup> {
    let name = name.trim();
    if name.is_empty() {
        bail!("分组名称不能为空");
    }
    if let Some(pid) = parent_id {
        get(conn, pid)?.context("父分组不存在")?;
    }
    let order = next_sort_order(conn, parent_id)?;
    conn.execute(
        "INSERT INTO sticker_groups (name, parent_id, sort_order) VALUES (?1, ?2, ?3)",
        params![name, parent_id, order],
    )
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

/// 设置分组颜色；None 或空串 = 清除颜色。
pub fn set_color(conn: &Connection, id: i64, color: Option<&str>) -> Result<()> {
    get(conn, id)?.context("分组不存在")?;
    let value = color.map(str::trim).filter(|c| !c.is_empty());
    conn.execute("UPDATE sticker_groups SET color = ?2 WHERE id = ?1", params![id, value])
        .context("设置分组颜色失败")?;
    Ok(())
}

/// 同级重排：按传入 id 顺序重写 sort_order（不属于该层级的 id 自动忽略）。
pub fn reorder(conn: &Connection, parent_id: Option<i64>, ids: &[i64]) -> Result<()> {
    let tx = conn.unchecked_transaction().context("开启重排事务失败")?;
    for (index, id) in ids.iter().enumerate() {
        tx.execute(
            "UPDATE sticker_groups SET sort_order = ?3 WHERE id = ?1 AND parent_id IS ?2",
            params![id, parent_id, index as i64],
        )
        .context("写入分组顺序失败")?;
    }
    tx.commit().context("提交重排事务失败")?;
    Ok(())
}

/// 把分组移动到新父级（None = 顶层），排在目标层级末尾。
/// 拒绝移动到自身或其子树内（否则形成环，整棵树将不可达）。
pub fn move_to_parent(conn: &Connection, id: i64, parent_id: Option<i64>) -> Result<()> {
    get(conn, id)?.context("分组不存在")?;
    if parent_id == Some(id) {
        bail!("不能把分组移动到它自己里面");
    }
    if let Some(pid) = parent_id {
        get(conn, pid)?.context("目标分组不存在")?;
        if is_descendant(conn, pid, id)? {
            bail!("不能把分组移动到它自己的子分组里");
        }
    }
    let order = next_sort_order(conn, parent_id)?;
    conn.execute(
        "UPDATE sticker_groups SET parent_id = ?2, sort_order = ?3 WHERE id = ?1",
        params![id, parent_id, order],
    )
    .context("移动分组失败")?;
    Ok(())
}

/// 删除分组。mode：
/// - "to-default"：组内便签由外键 ON DELETE SET NULL 自动回默认组；
/// - "with-stickers"：删除组内全部便签并返回其 id（调用方负责 md/assets 清理）。
///
/// 有子分组时一律拒绝：外键是 ON DELETE CASCADE，直接删会连带整棵子树，
/// 与「删除前先让用户处理子分组」的约定不符。
///
/// 全程单事务：任一步失败整体回滚，避免"便签已删、组还在"的中间态。
pub fn delete(conn: &Connection, id: i64, mode: &str) -> Result<Vec<i64>> {
    if mode != "to-default" && mode != "with-stickers" {
        bail!("未知的删除模式：{mode}");
    }
    get(conn, id)?.context("分组不存在")?;
    let children: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sticker_groups WHERE parent_id = ?1",
        params![id],
        |r| r.get(0),
    )?;
    if children > 0 {
        bail!("该分组下还有 {children} 个子分组，请先移动或删除它们");
    }

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

fn next_sort_order(conn: &Connection, parent_id: Option<i64>) -> Result<i64> {
    let next: i64 = conn.query_row(
        "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM sticker_groups WHERE parent_id IS ?1",
        params![parent_id],
        |r| r.get(0),
    )?;
    Ok(next)
}

/// candidate 是否位于 ancestor 的子树内（用于阻止「把分组拖进自己的子分组」）。
fn is_descendant(conn: &Connection, candidate: i64, ancestor: i64) -> Result<bool> {
    let mut current = Some(candidate);
    let mut guard = 0;
    while let Some(id) = current {
        guard += 1;
        if guard > 512 {
            break; // 防御异常数据（正常情况下不会出现环）
        }
        if id == ancestor {
            return Ok(true);
        }
        current = conn
            .query_row(
                "SELECT parent_id FROM sticker_groups WHERE id = ?1",
                params![id],
                |r| r.get::<_, Option<i64>>(0),
            )
            .optional()?
            .flatten();
    }
    Ok(false)
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
        let g = create(&c, "工作", None).unwrap();
        assert_eq!(g.name, "工作");
        assert_eq!(g.parent_id, None);
        assert_eq!(g.color, None);
        assert!(create(&c, "  ", None).is_err(), "空名拒绝");
        rename(&c, g.id, "学习").unwrap();
        assert_eq!(get(&c, g.id).unwrap().unwrap().name, "学习");
        assert!(list(&c).unwrap().iter().any(|x| x.id == g.id));
    }

    /// v19：分组的 parent_id / color 可读写（层级落库、颜色可清除）。
    #[test]
    fn group_tree_and_color_roundtrip() {
        let c = conn();
        let parent = create(&c, "学习", None).unwrap();
        let child = create(&c, "英语", Some(parent.id)).unwrap();
        assert_eq!(get(&c, child.id).unwrap().unwrap().parent_id, Some(parent.id));

        set_color(&c, parent.id, Some("#4F7CFF")).unwrap();
        assert_eq!(get(&c, parent.id).unwrap().unwrap().color.as_deref(), Some("#4F7CFF"));
        set_color(&c, parent.id, None).unwrap();
        assert_eq!(get(&c, parent.id).unwrap().unwrap().color, None, "清除颜色");
    }

    #[test]
    fn create_child_rejects_unknown_parent_and_goes_last() {
        let c = conn();
        assert!(create(&c, "x", Some(9999)).is_err(), "父分组不存在应拒绝");
        let p = create(&c, "P", None).unwrap();
        let a = create(&c, "A", Some(p.id)).unwrap();
        let b = create(&c, "B", Some(p.id)).unwrap();
        assert!(b.sort_order > a.sort_order, "新子分组排在末尾");
    }

    #[test]
    fn reorder_rewrites_sibling_order_only() {
        let c = conn();
        let a = create(&c, "A", None).unwrap();
        let b = create(&c, "B", None).unwrap();
        let child = create(&c, "child", Some(a.id)).unwrap();
        reorder(&c, None, &[b.id, a.id]).unwrap();
        assert!(get(&c, b.id).unwrap().unwrap().sort_order < get(&c, a.id).unwrap().unwrap().sort_order);
        assert_eq!(get(&c, child.id).unwrap().unwrap().sort_order, 0, "子分组顺序不受影响");
    }

    #[test]
    fn move_rejects_self_and_descendant_cycles() {
        let c = conn();
        let a = create(&c, "A", None).unwrap();
        let b = create(&c, "B", Some(a.id)).unwrap();
        let d = create(&c, "D", Some(b.id)).unwrap();
        assert!(move_to_parent(&c, a.id, Some(a.id)).is_err(), "不能移进自己");
        assert!(move_to_parent(&c, a.id, Some(d.id)).is_err(), "不能移进自己的后代");
        move_to_parent(&c, b.id, None).unwrap();
        assert_eq!(get(&c, b.id).unwrap().unwrap().parent_id, None);
        assert_eq!(get(&c, d.id).unwrap().unwrap().parent_id, Some(b.id), "子树随父移动");
    }

    #[test]
    fn delete_to_default_sets_null_and_with_stickers_removes() {
        let c = conn();
        let g = create(&c, "临时", None).unwrap();
        let s1 = make_sticker(&c);
        c.execute("UPDATE stickers SET group_id=?2 WHERE id=?1", params![s1, g.id]).unwrap();

        // to-default：FK SET NULL 自动回默认组
        delete(&c, g.id, "to-default").unwrap();
        let gid: Option<i64> = c.query_row("SELECT group_id FROM stickers WHERE id=?1", params![s1], |r| r.get(0)).unwrap();
        assert_eq!(gid, None);
        assert!(get(&c, g.id).unwrap().is_none());

        // with-stickers：返回并删除组内便签
        let g2 = create(&c, "危险", None).unwrap();
        let s2 = make_sticker(&c);
        c.execute("UPDATE stickers SET group_id=?2 WHERE id=?1", params![s2, g2.id]).unwrap();
        let removed = delete(&c, g2.id, "with-stickers").unwrap();
        assert_eq!(removed, vec![s2]);
        assert!(get(&c, g2.id).unwrap().is_none());
        let n: i64 = c.query_row("SELECT COUNT(*) FROM stickers", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1, "s1 还在（属于默认组）");
    }

    /// 有子分组时拒绝删除（否则外键 CASCADE 会连带删掉整棵子树）。
    #[test]
    fn delete_rejects_group_with_children() {
        let c = conn();
        let parent = create(&c, "父", None).unwrap();
        let child = create(&c, "子", Some(parent.id)).unwrap();
        let err = delete(&c, parent.id, "to-default").unwrap_err().to_string();
        assert!(err.contains("子分组"), "实际：{err}");
        assert!(get(&c, parent.id).unwrap().is_some());
        assert!(get(&c, child.id).unwrap().is_some(), "子树必须完好");
    }

    #[test]
    fn move_sticker_validates_target() {
        let c = conn();
        let s = make_sticker(&c);
        let g = create(&c, "G", None).unwrap();
        move_sticker(&c, s, Some(g.id)).unwrap();
        let gid: i64 = c.query_row("SELECT group_id FROM stickers WHERE id=?1", params![s], |r| r.get(0)).unwrap();
        assert_eq!(gid, g.id);
        move_sticker(&c, s, None).unwrap(); // 移出回默认组
        assert!(move_sticker(&c, s, Some(99999)).is_err(), "目标分组不存在应拒绝");
    }

    #[test]
    fn delete_rejects_unknown_mode_and_keeps_group() {
        let c = conn();
        let g = create(&c, "G", None).unwrap();
        assert!(delete(&c, g.id, "explode").is_err(), "未知模式应拒绝");
        assert!(get(&c, g.id).unwrap().is_some(), "未知模式不得删组");
    }
}
