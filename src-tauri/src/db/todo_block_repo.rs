//! 独立 Todo 块的 CRUD，保留 todo_items 作为旧 Markdown 任务实现。

use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::db::sticker_repo;
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
        "SELECT id, sticker_id, title, block_title, description, is_completed, parent_id, reminder_at, due_at, repeat_rule, sort_order, created_at, updated_at
         FROM todo_blocks WHERE id = ?1",
    )?;
    Ok(stmt.query_row(params![id], row_to_block).optional()?)
}

pub fn list_by_sticker(conn: &Connection, sticker_id: i64) -> Result<Vec<TodoBlock>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, sticker_id, title, block_title, description, is_completed, parent_id, reminder_at, due_at, repeat_rule, sort_order, created_at, updated_at
         FROM todo_blocks WHERE sticker_id = ?1
         ORDER BY parent_id IS NOT NULL, sort_order, created_at, id",
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
            title = COALESCE(?2, title), block_title = COALESCE(?3, block_title),
            description = COALESCE(?4, description),
            is_completed = COALESCE(?5, is_completed),
            reminder_at = CASE WHEN ?6 OR ?7 IS NULL THEN reminder_at ELSE NULLIF(?7, '') END,
            due_at = CASE WHEN ?8 OR ?9 IS NULL THEN due_at ELSE NULLIF(?9, '') END,
            repeat_rule = CASE WHEN ?10 OR ?11 IS NULL THEN repeat_rule ELSE NULLIF(?11, '') END,
            updated_at = datetime('now') WHERE id = ?1",
        params![
            id, patch.title, patch.block_title, patch.description, patch.is_completed.map(|v| v as i32),
            child, patch.reminder_at, child, patch.due_at, child, patch.repeat_rule,
        ],
    ).context("更新 Todo 块失败")?;
    get(conn, id)?.context("更新 Todo 块后读取失败")
}

/// 删除一个 Todo 任务（根任务即块本体，同时移除便签内容中的标记行）。
///
/// 约束：删除后该便签必须至少保留一个任务，否则拒绝（最后一个任务不可删）。
/// 删除根任务会连带 CASCADE 删除其子树。
pub fn delete(conn: &Connection, id: &str) -> Result<Option<i64>> {
    let block = get(conn, id)?.context("Todo 块不存在")?;
    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM todo_blocks WHERE sticker_id = ?1",
        params![block.sticker_id],
        |row| row.get(0),
    )?;
    let doom_count = subtree_count(conn, id)?;
    if total <= doom_count {
        bail!("每个 Todo 块至少保留一个任务，无法删除最后一项");
    }
    if block.parent_id.is_none() {
        remove_block_tag(conn, block.sticker_id, id)?;
    }
    conn.execute("DELETE FROM todo_blocks WHERE id = ?1", params![id])
        .context("删除 Todo 块失败")?;
    Ok(Some(block.sticker_id))
}

/// 计算以 `id` 为根的子树任务数（含自身，递归任意深度，兼容远期嵌套）。
fn subtree_count(conn: &Connection, id: &str) -> Result<i64> {
    Ok(conn.query_row(
        "WITH RECURSIVE sub(id) AS (
            SELECT id FROM todo_blocks WHERE id = ?1
            UNION ALL
            SELECT c.id FROM todo_blocks c JOIN sub s ON c.parent_id = s.id
        ) SELECT COUNT(*) FROM sub",
        params![id],
        |row| row.get(0),
    )?)
}

/// 收集便签内容中出现的 `<todo-block id="...">` 标记 id 集合。
pub fn tagged_ids(content: &str) -> Vec<String> {
    let mut ids = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("<todo-block") { continue; }
        if let Some(id) = extract_tag_id(trimmed) {
            ids.push(id);
        }
    }
    ids
}

fn extract_tag_id(tag: &str) -> Option<String> {
    if !tag.starts_with("<todo-block") { return None; }
    let quote = tag.find(&['"', '\''])?;
    let rest = &tag[quote + 1..];
    let end = rest.find(&['"', '\''])?;
    Some(rest[..end].to_string())
}

/// 从便签内容中移除对应 `<todo-block>` 标记行（幂等；仅改内容不改其他字段）。
fn remove_block_tag(conn: &Connection, sticker_id: i64, id: &str) -> Result<()> {
    let Some(sticker) = sticker_repo::get(conn, sticker_id)? else { return Ok(()) };
    let mut next = String::new();
    let mut changed = false;
    for line in sticker.content.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']).trim();
        if trimmed.starts_with("<todo-block") && extract_tag_id(trimmed).as_deref() == Some(id) {
            changed = true;
            continue;
        }
        next.push_str(line);
    }
    if changed {
        sticker_repo::update(conn, sticker_id, &sticker_repo::StickerPatch {
            content: Some(next),
            ..Default::default()
        })?;
    }
    Ok(())
}

/// 为「便签内容中无标记」的根任务补写 `<todo-block>` 标记（追加到内容末尾）。
/// 幂等：全部有标记时不写。返回本次补写标记的任务 id（供调用方通知）。
pub fn retag_orphans_for_sticker(conn: &Connection, sticker_id: i64) -> Result<Vec<String>> {
    let Some(sticker) = sticker_repo::get(conn, sticker_id)? else {
        return Ok(Vec::new());
    };
    let tagged = tagged_ids(&sticker.content);
    let blocks = list_by_sticker(conn, sticker_id)?;
    let orphans: Vec<String> = blocks
        .iter()
        .filter(|b| b.parent_id.is_none() && !tagged.iter().any(|id| id == &b.id))
        .map(|b| b.id.clone())
        .collect();
    if orphans.is_empty() {
        return Ok(orphans);
    }
    let mut next = sticker.content.trim_end().to_string();
    for id in &orphans {
        next.push_str("\n\n<todo-block id=\"");
        next.push_str(id);
        next.push_str("\"></todo-block>");
    }
    sticker_repo::update(conn, sticker_id, &sticker_repo::StickerPatch {
        content: Some(next),
        ..Default::default()
    })?;
    Ok(orphans)
}

/// 全局补写标记（启动时执行一次），返回每便签被补写的任务 id 列表。
pub fn retag_all_orphans(conn: &Connection) -> Result<Vec<(i64, Vec<String>)>> {
    let mut result = Vec::new();
    for sticker in sticker_repo::list_all(conn)? {
        let ids = retag_orphans_for_sticker(conn, sticker.id)?;
        if !ids.is_empty() {
            result.push((sticker.id, ids));
        }
    }
    Ok(result)
}

/// 重排任务顺序：`ids` 必须是同一分组（同为根或同为某父的直属子任务）
/// 且覆盖该组全部任务的完整新顺序。组内 sort_order 重新编号（0..n）。
pub fn reorder(conn: &Connection, ids: &[String]) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let first = get(conn, &ids[0])?.context("任务不存在")?;
    let group_of = |b: &TodoBlock| b.parent_id.clone();
    let group_key = group_of(&first);
    // 组内现有 id 集合（DB 顺序）
    let mut stmt = conn.prepare_cached(
        "SELECT id FROM todo_blocks
         WHERE sticker_id = ?1 AND (parent_id = ?2 OR (parent_id IS NULL AND ?2 IS NULL))
         ORDER BY sort_order, created_at, id",
    )?;
    let existing = stmt
        .query_map(params![first.sticker_id, group_key], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<String>>>()?;
    let mut sorted_db = existing.clone();
    sorted_db.sort();
    let mut sorted_in = ids.to_vec();
    sorted_in.sort();
    if sorted_db != sorted_in {
        bail!("排序列表必须为该分组全部任务且与当前顺序集合一致");
    }
    let tx = conn.unchecked_transaction()?;
    for (index, id) in ids.iter().enumerate() {
        tx.execute(
            "UPDATE todo_blocks SET sort_order = ?1, updated_at = datetime('now') WHERE id = ?2",
            params![index as i64, id],
        )
        .context("更新任务排序失败")?;
    }
    tx.commit()?;
    Ok(())
}

fn row_to_block(row: &rusqlite::Row<'_>) -> rusqlite::Result<TodoBlock> {
    Ok(TodoBlock {
        id: row.get(0)?, sticker_id: row.get(1)?, title: row.get(2)?, block_title: row.get(3)?,
        description: row.get(4)?, is_completed: row.get(5)?, parent_id: row.get(6)?, reminder_at: row.get(7)?,
        due_at: row.get(8)?, repeat_rule: row.get(9)?, sort_order: row.get(10)?, created_at: row.get(11)?, updated_at: row.get(12)?,
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
    fn creates_parent_and_multiple_direct_children() {
        let conn = conn();
        let sticker_id = sticker(&conn);
        let parent = create(&conn, sticker_id, None).unwrap();
        let child_one = create(&conn, sticker_id, Some(&parent.id)).unwrap();
        let child_two = create(&conn, sticker_id, Some(&parent.id)).unwrap();
        assert_eq!(child_one.parent_id.as_deref(), Some(parent.id.as_str()));
        assert_eq!(child_two.parent_id.as_deref(), Some(parent.id.as_str()));
        assert!(create(&conn, sticker_id, Some(&child_one.id)).is_err());
    }

    #[test]
    fn child_cannot_write_advanced_fields_and_delete_keeps_block_alive() {
        let conn = conn();
        let sticker_id = sticker(&conn);
        let parent = create(&conn, sticker_id, None).unwrap();
        let child = create(&conn, sticker_id, Some(&parent.id)).unwrap();
        let updated = update(&conn, &child.id, &TodoPatch { reminder_at: Some("2030-01-01T00:00:00Z".into()), ..Default::default() }).unwrap();
        assert!(updated.reminder_at.is_none());
        delete(&conn, &child.id).unwrap();
        assert!(get(&conn, &child.id).unwrap().is_none());
        assert!(get(&conn, &parent.id).unwrap().is_some());
    }

    #[test]
    fn deleting_last_task_is_rejected_but_dropping_tree_leaves_others() {
        let conn = conn();
        let sticker_id = sticker(&conn);
        // 便签仅一个块：删除（最后一项）被拒绝
        let parent = create(&conn, sticker_id, None).unwrap();
        let child = create(&conn, sticker_id, Some(&parent.id)).unwrap();
        assert!(delete(&conn, &child.id).is_ok(), "子任务删除后便签仍有任务，应成功");
        // 便签仅一个块（含子任务）：删除根即清空整块 → 拒绝
        assert!(delete(&conn, &parent.id).is_err(), "删除便签最后一个任务被拒绝");
        assert!(get(&conn, &parent.id).unwrap().is_some());
        // 第二个块存在时，删除第一个块（整棵树）允许，便签仍有任务
        let second = create(&conn, sticker_id, None).unwrap();
        let first = create(&conn, sticker_id, None).unwrap();
        delete(&conn, &first.id).unwrap();
        assert!(get(&conn, &first.id).unwrap().is_none());
        assert!(get(&conn, &second.id).unwrap().is_some());
    }

    #[test]
    fn deleting_root_removes_tag_line_from_sticker_content() {
        let conn = conn();
        let sticker_id = sticker(&conn);
        // 构造：便签内容里有两个标记行，仅删其中一块的标记
        let a = create(&conn, sticker_id, None).unwrap();
        let b = create(&conn, sticker_id, None).unwrap();
        let content = format!("# 便签\n\n<todo-block id=\"{}\"></todo-block>\n\n正文\n\n<todo-block id='{}'></todo-block>\n", a.id, b.id);
        sticker_repo::update(&conn, sticker_id, &sticker_repo::StickerPatch { content: Some(content), ..Default::default() }).unwrap();
        delete(&conn, &a.id).unwrap();
        let after = sticker_repo::get(&conn, sticker_id).unwrap().unwrap().content;
        assert!(!after.contains(&a.id));
        assert!(after.contains(&b.id));
        assert!(after.contains("正文"));
    }

    #[test]
    fn reorder_renumbers_group_and_rejects_cross_group() {
        let conn = conn();
        let sticker_id = sticker(&conn);
        let root_a = create(&conn, sticker_id, None).unwrap();
        let root_b = create(&conn, sticker_id, None).unwrap();
        let child_a1 = create(&conn, sticker_id, Some(&root_a.id)).unwrap();
        let child_a2 = create(&conn, sticker_id, Some(&root_a.id)).unwrap();
        // 根组重排：反转两个根
        reorder(&conn, &[root_b.id.clone(), root_a.id.clone()]).unwrap();
        let listed = list_by_sticker(&conn, sticker_id).unwrap();
        let root_ids: Vec<&str> = listed.iter().filter(|b| b.parent_id.is_none()).map(|b| b.id.as_str()).collect();
        assert_eq!(root_ids, vec![root_b.id.as_str(), root_a.id.as_str()]);
        // 子任务组重排
        reorder(&conn, &[child_a2.id.clone(), child_a1.id.clone()]).unwrap();
        let listed = list_by_sticker(&conn, sticker_id).unwrap();
        let child_ids: Vec<&str> = listed.iter().filter(|b| b.parent_id.as_deref() == Some(root_a.id.as_str())).map(|b| b.id.as_str()).collect();
        assert_eq!(child_ids, vec![child_a2.id.as_str(), child_a1.id.as_str()]);
        // 跨组（根+子混合）被拒绝
        assert!(reorder(&conn, &[root_a.id.clone(), child_a1.id.clone()]).is_err());
        // 集合不完整被拒绝
        assert!(reorder(&conn, &[root_b.id.clone()]).is_err());
    }

    #[test]
    fn block_title_update_roundtrip() {
        let conn = conn();
        let sticker_id = sticker(&conn);
        let block = create(&conn, sticker_id, None).unwrap();
        assert_eq!(block.block_title, "");
        let updated = update(&conn, &block.id, &TodoPatch { block_title: Some("我的块".into()), ..Default::default() }).unwrap();
        assert_eq!(updated.block_title, "我的块");
    }

    #[test]
    fn retag_orphans_appends_tags_for_untagged_roots_only() {
        let conn = conn();
        let sticker_id = sticker(&conn);
        let a = create(&conn, sticker_id, None).unwrap();
        let a_child = create(&conn, sticker_id, Some(&a.id)).unwrap();
        let b = create(&conn, sticker_id, None).unwrap();
        let covered = format!("<todo-block id=\"{}\"></todo-block>\n", b.id);
        sticker_repo::update(&conn, sticker_id, &sticker_repo::StickerPatch { content: Some(covered), ..Default::default() }).unwrap();
        let retagged = retag_orphans_for_sticker(&conn, sticker_id).unwrap();
        assert_eq!(retagged, vec![a.id.clone()]);
        let after = sticker_repo::get(&conn, sticker_id).unwrap().unwrap().content;
        assert!(after.contains(&format!("<todo-block id=\"{}\"></todo-block>", a.id)));
        assert!(get(&conn, &a.id).unwrap().is_some());
        assert!(get(&conn, &a_child.id).unwrap().is_some());
        assert!(get(&conn, &b.id).unwrap().is_some());
        // 幂等：再次调用不再写入、不重复追加
        let again = retag_orphans_for_sticker(&conn, sticker_id).unwrap();
        assert!(again.is_empty());
        assert_eq!(sticker_repo::get(&conn, sticker_id).unwrap().unwrap().content, after);
    }
}
