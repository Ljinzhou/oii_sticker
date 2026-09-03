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

/// 节点层级：0 = 块（根），1 = 父任务，2 = 子任务。
///
/// 结构固定为三层：`块 → 父任务 → 子任务`。
/// 块由编辑器 `/` 菜单创建（parent_id = NULL），自身不作为任务显示；
/// 父任务由 Todo 窗口「新建任务」创建；子任务只能挂在父任务下。
pub fn depth_of(conn: &Connection, id: &str) -> Result<usize> {
    let mut depth = 0usize;
    let mut cursor = get(conn, id)?;
    while let Some(node) = cursor {
        let Some(parent) = node.parent_id.clone() else { break };
        depth += 1;
        cursor = get(conn, &parent)?;
        // 兜底：数据异常成环时停住，避免死循环
        if depth > 16 { break; }
    }
    Ok(depth)
}

/// 创建新节点。
///
/// 层级约束（三层结构）：
///   - `parent_id = None`        → 新建**块**（第 0 层）
///   - parent 为块（第 0 层）     → 新建**父任务**（第 1 层）
///   - parent 为父任务（第 1 层） → 新建**子任务**（第 2 层）
///   - parent 为子任务（第 2 层） → 拒绝（子任务下不能再挂）
pub fn create(conn: &Connection, sticker_id: i64, parent_id: Option<&str>) -> Result<TodoBlock> {
    if let Some(parent_id) = parent_id {
        let parent = get(conn, parent_id)?.context("父任务不存在")?;
        if parent.sticker_id != sticker_id {
            bail!("任务必须属于同一便签");
        }
        let parent_depth = depth_of(conn, parent_id)?;
        if parent_depth >= 2 {
            bail!("最多两层：子任务下不能再添加子任务");
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
        "SELECT id, sticker_id, title, block_title, description, is_completed, parent_id, reminder_at, due_at, repeat_rule, reminded_at, due_notified_at, reminder_ack_at, due_ack_at, sort_order, created_at, updated_at, completed_at, repeat_anchor
         FROM todo_blocks WHERE id = ?1",
    )?;
    Ok(stmt.query_row(params![id], row_to_block).optional()?)
}

pub fn list_by_sticker(conn: &Connection, sticker_id: i64) -> Result<Vec<TodoBlock>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, sticker_id, title, block_title, description, is_completed, parent_id, reminder_at, due_at, repeat_rule, reminded_at, due_notified_at, reminder_ack_at, due_ack_at, sort_order, created_at, updated_at, completed_at, repeat_anchor
         FROM todo_blocks WHERE sticker_id = ?1
         ORDER BY parent_id IS NOT NULL, sort_order, created_at, id",
    )?;
    let rows = stmt
        .query_map(params![sticker_id], row_to_block)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn update(conn: &Connection, id: &str, patch: &TodoPatch) -> Result<TodoBlock> {
    let _current = get(conn, id)?.context("Todo 块不存在")?;
    // 只有第 2 层「子任务」受限（仅名称+备注）；
    // 第 1 层「父任务」与本块本身可正常写提醒/截止/重复。
    // 注意：不能用 parent_id.is_some() 判断——那会把父任务也误判成子任务。
    let child = depth_of(conn, id)? >= 2;
    let is_repeat_cleared = matches!(patch.repeat_rule.as_deref(), Some(""));
    // repeat_rule 变更时按规则计算下一个重建日（本地日期）；清空/未动则保持。
    let next_anchor = patch.repeat_rule.as_deref().and_then(|rule| {
        if rule.is_empty() {
            None
        } else {
            compute_repeat_anchor(rule)
        }
    });
    conn.execute(
        "UPDATE todo_blocks SET
            title = COALESCE(?2, title), block_title = COALESCE(?3, block_title),
            description = COALESCE(?4, description),
            is_completed = COALESCE(?5, is_completed),
            -- 完成时刻：翻转为完成时写入本地时间；取消完成时清空；未动时保持原值。
            completed_at = CASE
                WHEN ?5 = 1 THEN datetime('now', 'localtime')
                WHEN ?5 = 0 THEN NULL
                ELSE completed_at END,
            reminder_at = CASE WHEN ?6 OR ?7 IS NULL THEN reminder_at ELSE NULLIF(?7, '') END,
            due_at = CASE WHEN ?8 OR ?9 IS NULL THEN due_at ELSE NULLIF(?9, '') END,
            repeat_rule = CASE WHEN ?10 OR ?11 IS NULL THEN repeat_rule ELSE NULLIF(?11, '') END,
            -- 重复规则变更 → 按规则重置重建锚点（NULL=清空/退出重建队列）。
            repeat_anchor = CASE
                WHEN ?13 THEN NULL
                WHEN ?10 OR ?11 IS NULL THEN repeat_anchor
                ELSE ?12 END,
            -- 重新设置提醒/截止（或完成任务）时重置触发与确认标记，调度器重新武装：
            --   提醒时间变化 → 清 reminded_at + reminder_ack_at；
            --   截止变化 → 清 due_notified_at + due_ack_at；
            --   完成任务 → 全部清空（已完成任务不再提醒/高亮）。
            reminded_at = CASE
                WHEN ?5 = 1 THEN NULL
                WHEN ?6 OR ?7 IS NULL THEN reminded_at
                ELSE NULL END,
            reminder_ack_at = CASE
                WHEN ?5 = 1 THEN NULL
                WHEN ?6 OR ?7 IS NULL THEN reminder_ack_at
                ELSE NULL END,
            due_notified_at = CASE
                WHEN ?5 = 1 THEN NULL
                WHEN ?8 OR ?9 IS NULL THEN due_notified_at
                ELSE NULL END,
            due_ack_at = CASE
                WHEN ?5 = 1 THEN NULL
                WHEN ?8 OR ?9 IS NULL THEN due_ack_at
                ELSE NULL END,
            updated_at = datetime('now') WHERE id = ?1",
        params![
            id, patch.title, patch.block_title, patch.description, patch.is_completed.map(|v| v as i32),
            child, patch.reminder_at, child, patch.due_at, child, patch.repeat_rule,
            next_anchor, is_repeat_cleared,
        ],
    ).context("更新 Todo 块失败")?;
    get(conn, id)?.context("更新 Todo 块后读取失败")
}

/// 确认收到提醒（红点点击）：清除已触发标记（高亮消失）并记录确认时刻。
///
/// 只对「已触发」的字段写确认时刻，避免把未触发的未来提醒误标记为已读。
/// 调度器据此保证：确认后同一提醒不再触发（重启也不会再提示），
/// 而「晚于确认时刻」的新设置提醒 / 循环任务后续周期照常触发。
pub fn ack_alerts(conn: &Connection, id: &str) -> Result<Option<TodoBlock>> {
    conn.execute(
        "UPDATE todo_blocks SET
            reminder_ack_at = CASE WHEN reminded_at IS NOT NULL THEN datetime('now') ELSE reminder_ack_at END,
            due_ack_at      = CASE WHEN due_notified_at IS NOT NULL THEN datetime('now') ELSE due_ack_at END,
            reminded_at     = NULL,
            due_notified_at = NULL,
            updated_at      = datetime('now')
         WHERE id = ?1",
        params![id],
    )
    .context("写入提醒确认失败")?;
    get(conn, id)
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

/// 删除一个便签中**无 `<todo-block>` 标签对应**的根任务（以及 CASCADE 子树）。
///
/// 应用场景：用户手动在 Markdown 源码中删除了若干 `<todo-block>` 标签并保存，
/// 需要把 DB 里对应的根任务清理掉（连同子任务），防止"标签没了但任务仍在，
/// Todo 窗口与便签渲染状态错位"（"删除标签后任务复活"的 bug 修复）。
///
/// 与 `delete` 的差异：
///   - 不调 `remove_block_tag`（标签已无）；
///   - 不校验"每个块至少保留一个任务"——用户删标签是显式意图，全删也允许；
///   - 仅作用于指定 `sticker_id` 的根任务。
///
/// 返回被删除的根任务 id 列表（按 DB 顺序）。
pub fn delete_roots_not_in(conn: &Connection, sticker_id: i64, tag_ids: &[String]) -> Result<Vec<String>> {
    let blocks = list_by_sticker(conn, sticker_id)?;
    let mut doomed: Vec<String> = Vec::new();
    for b in &blocks {
        if b.parent_id.is_some() { continue; } // 只处理根任务，子任务靠 CASCADE
        if tag_ids.iter().any(|t| t == &b.id) { continue; }
        doomed.push(b.id.clone());
    }
    if doomed.is_empty() { return Ok(doomed); }
    let tx = conn.unchecked_transaction()?;
    for id in &doomed {
        tx.execute("DELETE FROM todo_blocks WHERE id = ?1", params![id])
            .context("删除孤儿根任务失败")?;
    }
    tx.commit()?;
    Ok(doomed)
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
        due_at: row.get(8)?, repeat_rule: row.get(9)?, reminded_at: row.get(10)?, due_notified_at: row.get(11)?,
        reminder_ack_at: row.get(12)?, due_ack_at: row.get(13)?,
        sort_order: row.get(14)?, created_at: row.get(15)?, updated_at: row.get(16)?,
        completed_at: row.get(17)?, repeat_anchor: row.get(18)?,
    })
}

// ═══════════════════ 重复任务：重建锚点与逾期后缀 ═══════════════════

/// 当前本地日期（"YYYY-MM-DD"）。
pub fn local_date_str() -> String {
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    now.date().to_string()
}

/// Unix 毫秒 → 本地日期（"YYYY-MM-DD"）。
pub fn local_date_of(ms: i64) -> Option<String> {
    let dt = time::OffsetDateTime::from_unix_timestamp(ms.div_euclid(1000)).ok()?;
    let local = dt.to_offset(time::UtcOffset::current_local_offset().ok()?);
    Some(local.date().to_string())
}

/// "YYYY-MM-DD" → 前一天（跨月/跨年安全，纯字符串运算）。
pub fn previous_day_str(date: &str) -> Option<String> {
    let parts: Vec<i64> = date.split('-').filter_map(|p| p.parse().ok()).collect();
    if parts.len() != 3 {
        return None;
    }
    let (mut y, mut m, mut d) = (parts[0], parts[1], parts[2]);
    if d <= 1 {
        m -= 1;
        if m == 0 {
            y -= 1;
            m = 12;
        }
        d = days_in_month(y, m);
    } else {
        d -= 1;
    }
    Some(format!("{y:04}-{m:02}-{d:02}"))
}

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

/// 重复任务下一个重建日（本地 "YYYY-MM-DD"）：按规则从当前时间推进。
///
/// 复用 `reminder::next_occurrence_ms` 的 day/week/month/year 推进语义：
/// `next_occurrence_ms(rule, base_ms, base_ms)` 返回严格晚于 base 的下一次时点，
/// 取其本地日期即锚点。
pub fn compute_repeat_anchor(rule: &str) -> Option<String> {
    let now_ms = {
        let now = time::OffsetDateTime::now_local().ok()?;
        now.unix_timestamp() * 1000
    };
    let next_ms = crate::reminder::next_occurrence_ms(rule, now_ms, now_ms)?;
    local_date_of(next_ms)
}

/// 逾期改名的固定后缀（与前端 `presets.ts` 的 OVERDUE_SUFFIX_RE 对应）。
pub const OVERDUE_SUFFIX: &str = "，任务逾期";

/// 标题是否已带逾期后缀（幂等改名检查）。
pub fn has_overdue_suffix(title: &str) -> bool {
    strip_overdue_suffix(title) != title
}

/// 剥离标题末尾的「——YYYY年M月D日，任务逾期」后缀，返回原始标题。
pub fn strip_overdue_suffix(title: &str) -> String {
    if let Some(pos) = title.rfind("——") {
        let tail = &title[pos + "——".len()..];
        if let Some(rest) = tail.strip_suffix(OVERDUE_SUFFIX) {
            if is_cn_date(rest) {
                return title[..pos].trim_end().to_string();
            }
        }
    }
    title.to_string()
}

/// 校验 "YYYY年M月D日"。
fn is_cn_date(s: &str) -> bool {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() < 9 {
        return false;
    }
    if !chars[..4].iter().all(|c| c.is_ascii_digit()) || chars[4] != '年' {
        return false;
    }
    let mut i = 5;
    let mut month = String::new();
    while i < chars.len() && chars[i].is_ascii_digit() {
        month.push(chars[i]);
        i += 1;
    }
    if month.is_empty() || i >= chars.len() || chars[i] != '月' {
        return false;
    }
    i += 1;
    let mut day = String::new();
    while i < chars.len() && chars[i].is_ascii_digit() {
        day.push(chars[i]);
        i += 1;
    }
    if day.is_empty() || i + 1 != chars.len() || chars[i] != '日' {
        return false;
    }
    true
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
    }

    /// 三层结构：块(0) → 父任务(1) → 子任务(2)，第四层被拒绝。
    #[test]
    fn three_level_nesting_allowed_but_fourth_rejected() {
        let conn = conn();
        let sticker_id = sticker(&conn);
        let block = create(&conn, sticker_id, None).unwrap();
        assert_eq!(depth_of(&conn, &block.id).unwrap(), 0, "parent_id=NULL 的是块（第 0 层）");

        // 第 1 层：父任务（挂在块下）
        let task = create(&conn, sticker_id, Some(&block.id)).unwrap();
        assert_eq!(depth_of(&conn, &task.id).unwrap(), 1);

        // 第 2 层：子任务（挂在父任务下）
        let sub = create(&conn, sticker_id, Some(&task.id)).unwrap();
        assert_eq!(depth_of(&conn, &sub.id).unwrap(), 2);

        // 第 3 层：子任务下不能再挂 —— 拒绝
        let err = create(&conn, sticker_id, Some(&sub.id)).unwrap_err().to_string();
        assert!(err.contains("最多两层"), "应拒绝第四层，实际错误：{err}");

        // 跨便签仍然拒绝
        let other = sticker(&conn);
        assert!(create(&conn, other, Some(&task.id)).is_err());
    }

    /// 三层下「谁能写提醒」：
    ///   - 父任务（第 1 层）→ 可以写提醒/截止/重复
    ///   - 子任务（第 2 层）→ 一律忽略，只能改名称与备注
    #[test]
    fn parent_task_can_set_reminder_but_subtask_cannot() {
        let conn = conn();
        let sticker_id = sticker(&conn);
        let block = create(&conn, sticker_id, None).unwrap();
        let task = create(&conn, sticker_id, Some(&block.id)).unwrap();
        let sub = create(&conn, sticker_id, Some(&task.id)).unwrap();

        // 父任务：提醒/截止可写
        let t = update(&conn, &task.id, &TodoPatch {
            reminder_at: Some("2030-01-01T00:00:00Z".into()),
            due_at: Some("2030-01-02T00:00:00Z".into()),
            ..Default::default()
        }).unwrap();
        assert_eq!(t.reminder_at.as_deref(), Some("2030-01-01T00:00:00Z"));
        assert_eq!(t.due_at.as_deref(), Some("2030-01-02T00:00:00Z"));

        // 子任务：提醒/截止被忽略，但名称与备注可写
        let s = update(&conn, &sub.id, &TodoPatch {
            title: Some("子任务改名".into()),
            description: Some("子任务备注".into()),
            reminder_at: Some("2030-01-01T00:00:00Z".into()),
            due_at: Some("2030-01-02T00:00:00Z".into()),
            repeat_rule: Some("daily".into()),
            ..Default::default()
        }).unwrap();
        assert_eq!(s.title, "子任务改名", "子任务可改名称");
        assert_eq!(s.description.as_deref(), Some("子任务备注"), "子任务可写备注");
        assert!(s.reminder_at.is_none(), "子任务不能设提醒");
        assert!(s.due_at.is_none(), "子任务不能设截止");
        assert!(s.repeat_rule.is_none(), "子任务不能设重复");
    }

    #[test]
    fn delete_child_keeps_block_alive() {
        let conn = conn();
        let sticker_id = sticker(&conn);
        let parent = create(&conn, sticker_id, None).unwrap();
        let child = create(&conn, sticker_id, Some(&parent.id)).unwrap();
        delete(&conn, &child.id).unwrap();
        assert!(get(&conn, &child.id).unwrap().is_none());
        assert!(get(&conn, &parent.id).unwrap().is_some());
    }

    /// 提醒触发标记的生命周期：改提醒时间/完成任务时重置，未涉及更新保持。
    #[test]
    fn update_resets_trigger_flags_on_rearm_and_complete() {
        let conn = conn();
        let sticker_id = sticker(&conn);
        let block = create(&conn, sticker_id, None).unwrap();
        // 模拟调度器已触发：直接 SQL 写入触发标记
        conn.execute(
            "UPDATE todo_blocks SET reminded_at = '2026-08-25T08:00:00Z', due_notified_at = '2026-08-25T09:00:00Z' WHERE id = ?1",
            params![block.id],
        ).unwrap();

        // 无关字段更新（改标题）：触发标记保持（高亮仍有效）
        let keep = update(&conn, &block.id, &TodoPatch { title: Some("改名".into()), ..Default::default() }).unwrap();
        assert_eq!(keep.reminded_at.as_deref(), Some("2026-08-25T08:00:00Z"));
        assert_eq!(keep.due_notified_at.as_deref(), Some("2026-08-25T09:00:00Z"));

        // 重新设置提醒时间：reminded_at 重置（重新武装），due 标记不受影响
        let rearmed = update(&conn, &block.id, &TodoPatch { reminder_at: Some("2026-08-26T10:00:00Z".into()), ..Default::default() }).unwrap();
        assert!(rearmed.reminded_at.is_none(), "重设提醒应清除已触发标记");
        assert_eq!(rearmed.due_notified_at.as_deref(), Some("2026-08-25T09:00:00Z"));

        // 完成任务：两个触发标记都清空
        let done = update(&conn, &block.id, &TodoPatch { is_completed: Some(true), ..Default::default() }).unwrap();
        assert!(done.reminded_at.is_none() && done.due_notified_at.is_none(), "完成任务应清除全部触发标记");
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

    /// 完成时刻（completed_at）生命周期：完成时写入本地时间、
    /// 无关更新保持原值、取消完成时清空（供已完成任务块展示）。
    #[test]
    fn completed_at_roundtrip_on_complete_and_undo() {
        let conn = conn();
        let sticker_id = sticker(&conn);
        let block = create(&conn, sticker_id, None).unwrap();
        assert!(block.completed_at.is_none(), "新任务无完成时刻");

        let done = update(&conn, &block.id, &TodoPatch { is_completed: Some(true), ..Default::default() }).unwrap();
        let stamp = done.completed_at.clone().expect("完成应写入 completed_at");
        assert_eq!(stamp.len(), 19, "datetime('now','localtime') 形如 YYYY-MM-DD HH:MM:SS");

        // 无关字段更新：完成时刻保持不变（程序不得擅改用户数据）
        let keep = update(&conn, &block.id, &TodoPatch { title: Some("改名".into()), ..Default::default() }).unwrap();
        assert_eq!(keep.completed_at.as_deref(), Some(stamp.as_str()));

        // 取消完成：清空
        let undone = update(&conn, &block.id, &TodoPatch { is_completed: Some(false), ..Default::default() }).unwrap();
        assert!(undone.completed_at.is_none());
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

    /// 核心 bug 修复：用户删除 markdown 中的 `<todo-block>` 标签并保存后，
    /// `delete_roots_not_in` 必须把对应的根任务（连同子任务）从 DB 中清掉，
    /// 防止"标签没了但任务仍在"导致 Todo 窗口与便签渲染错位。
    #[test]
    fn delete_roots_not_in_removes_tagged_missing_roots_and_cascades() {
        let conn = conn();
        let sticker_id = sticker(&conn);

        // 3 个根任务 + 各自的子任务
        let a = create(&conn, sticker_id, None).unwrap();
        let a_child = create(&conn, sticker_id, Some(&a.id)).unwrap();
        let b = create(&conn, sticker_id, None).unwrap();
        let b_child = create(&conn, sticker_id, Some(&b.id)).unwrap();
        let c = create(&conn, sticker_id, None).unwrap();

        // 当前 markdown 仅保留 a 的标签（b、c 的标签被用户手动删了）
        let content = format!("<todo-block id=\"{}\"></todo-block>\n", a.id);
        sticker_repo::update(&conn, sticker_id, &sticker_repo::StickerPatch {
            content: Some(content),
            ..Default::default()
        }).unwrap();

        // 收集"无标签"的根 → 应是 b、c
        let doomed = delete_roots_not_in(&conn, sticker_id, &[a.id.clone()]).unwrap();
        assert_eq!(doomed.len(), 2);
        assert!(doomed.contains(&b.id));
        assert!(doomed.contains(&c.id));
        assert!(!doomed.contains(&a.id), "a 有标签，不应被删");

        // b、c 行已被删，子任务靠 CASCADE 一并清掉
        assert!(get(&conn, &a.id).unwrap().is_some(), "a 应保留");
        assert!(get(&conn, &a_child.id).unwrap().is_some(), "a 的子任务应保留");
        assert!(get(&conn, &b.id).unwrap().is_none(), "b 应被删");
        assert!(get(&conn, &b_child.id).unwrap().is_none(), "b 的子任务应被级联删");
        assert!(get(&conn, &c.id).unwrap().is_none(), "c 应被删");

        // 幂等：再调一次无新对象被删
        let again = delete_roots_not_in(&conn, sticker_id, &[a.id.clone()]).unwrap();
        assert!(again.is_empty(), "无可删时返回空列表");
    }

    /// 用户清空 content（保存空便签）→ 所有根任务全部被清。
    /// 这与"用户显式删除所有标签"的语义一致：标签都没了，块也没有存在的理由。
    #[test]
    fn delete_roots_not_in_with_empty_tag_list_clears_all_roots() {
        let conn = conn();
        let sticker_id = sticker(&conn);
        let a = create(&conn, sticker_id, None).unwrap();
        let b = create(&conn, sticker_id, None).unwrap();
        // 用户保存了空便签（清空 content）
        sticker_repo::update(&conn, sticker_id, &sticker_repo::StickerPatch {
            content: Some(String::new()),
            ..Default::default()
        }).unwrap();
        let doomed = delete_roots_not_in(&conn, sticker_id, &[]).unwrap();
        assert_eq!(doomed.len(), 2);
        assert!(get(&conn, &a.id).unwrap().is_none());
        assert!(get(&conn, &b.id).unwrap().is_none());
    }

    /// 没有任何根任务时调用不报错，返回空列表。
    #[test]
    fn delete_roots_not_in_is_noop_when_no_blocks() {
        let conn = conn();
        let sticker_id = sticker(&conn);
        let doomed = delete_roots_not_in(&conn, sticker_id, &[]).unwrap();
        assert!(doomed.is_empty());
    }

    /// ack_alerts 端到端：调度线程标记触发（reminder + due 均已触发）后，
    /// 用户确认 → 清除触发标记（前端高亮消失）并记录确认时刻；
    /// 确认后再次 ack 幂等，不报错、不改状态。
    #[test]
    fn ack_alerts_clears_fired_flags_and_records_ack_timestamps() {
        let conn = conn();
        let sticker_id = sticker(&conn);
        let block = create(&conn, sticker_id, None).unwrap();
        let task = create(&conn, sticker_id, Some(&block.id)).unwrap();

        update(&conn, &task.id, &TodoPatch {
            reminder_at: Some("2030-01-01T00:00:00Z".into()),
            due_at: Some("2030-01-02T00:00:00Z".into()),
            ..Default::default()
        })
        .unwrap();

        // 模拟调度线程 fire_one 写入触发标记（reminded_at / due_notified_at）
        conn.execute(
            "UPDATE todo_blocks SET reminded_at = ?2, due_notified_at = ?2 WHERE id = ?1",
            params![task.id, "2030-01-01T00:00:00Z"],
        )
        .unwrap();

        let acked = ack_alerts(&conn, &task.id).unwrap().unwrap();
        assert_eq!(acked.reminded_at, None, "确认后提醒触发标记应清除（高亮消失）");
        assert_eq!(acked.due_notified_at, None, "确认后截止触发标记应清除");
        assert!(acked.reminder_ack_at.is_some(), "已触发的提醒应记录确认时刻");
        assert!(acked.due_ack_at.is_some(), "已触发的截止应记录确认时刻");

        let again = ack_alerts(&conn, &task.id).unwrap().unwrap();
        assert_eq!(again.reminded_at, None);
        assert_eq!(again.due_notified_at, None);
        assert!(again.reminder_ack_at.is_some(), "重复确认不丢失已记确认时刻");
    }

    /// ack 只对「已触发」的字段写确认时刻：
    /// 仅提醒触发、截止尚未到时 → 截止不得被误标为已读，未来 due 保留照常触发。
    #[test]
    fn ack_alerts_does_not_ack_unfired_future_fields() {
        let conn = conn();
        let sticker_id = sticker(&conn);
        let block = create(&conn, sticker_id, None).unwrap();
        let task = create(&conn, sticker_id, Some(&block.id)).unwrap();
        update(&conn, &task.id, &TodoPatch {
            reminder_at: Some("2030-01-01T00:00:00Z".into()),
            due_at: Some("2030-01-02T00:00:00Z".into()),
            ..Default::default()
        })
        .unwrap();

        // 只有提醒触发，截止尚未触发
        conn.execute(
            "UPDATE todo_blocks SET reminded_at = ?2 WHERE id = ?1",
            params![task.id, "2030-01-01T00:00:00Z"],
        )
        .unwrap();

        let acked = ack_alerts(&conn, &task.id).unwrap().unwrap();
        assert!(acked.reminder_ack_at.is_some(), "已触发的提醒应记录确认时刻");
        assert_eq!(acked.due_ack_at, None, "未触发的未来截止不得被误标为已确认");
        assert!(acked.due_at.is_some(), "未来截止时间必须保留，后续照常触发");
    }

    /// ack 对未设置提醒的任务无副作用：直接返回原块。
    #[test]
    fn ack_alerts_is_noop_for_task_without_reminders() {
        let conn = conn();
        let sticker_id = sticker(&conn);
        let block = create(&conn, sticker_id, None).unwrap();
        let task = create(&conn, sticker_id, Some(&block.id)).unwrap();

        let acked = ack_alerts(&conn, &task.id).unwrap().unwrap();
        assert_eq!(acked.reminded_at, None);
        assert_eq!(acked.due_notified_at, None);
        assert_eq!(acked.reminder_ack_at, None);
        assert_eq!(acked.due_ack_at, None);
    }

    /// repeat_anchor：按规则计算下一个重建日（day=明天；week 带星期=下个匹配；month=下月同日）。
    #[test]
    fn compute_repeat_anchor_advances_by_rule() {
        use std::ops::Add;
        use time::OffsetDateTime;
        let today = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc()).date();
        let tomorrow = today.next_day().unwrap().to_string();
        assert_eq!(
            compute_repeat_anchor(r#"{"unit":"day","interval":1}"#).as_deref(),
            Some(tomorrow.as_str()),
        );
        assert_eq!(
            compute_repeat_anchor(r#"{"unit":"day","interval":2}"#).as_deref(),
            Some(today.next_day().unwrap().next_day().unwrap().to_string().as_str()),
        );
        // week（无 weekdays）= 7 天后
        assert_eq!(
            compute_repeat_anchor(r#"{"unit":"week","interval":1}"#).as_deref(),
            Some(today.next_day().unwrap().add(time::Duration::days(6)).to_string().as_str()),
        );
        // month = 下一个月的同日（年/月推进；日由 next_occurrence_ms 钳制）
        let next_month_first = today
            .replace_day(1)
            .unwrap()
            .add(time::Duration::days(32))
            .replace_day(1)
            .unwrap();
        let got_month = compute_repeat_anchor(r#"{"unit":"month","interval":1}"#).unwrap();
        let gm: Vec<u32> = got_month.split('-').filter_map(|p| p.parse().ok()).collect();
        assert_eq!(gm.len(), 3);
        assert_eq!(gm[0], next_month_first.year() as u32);
        assert_eq!(gm[1], u8::from(next_month_first.month()) as u32);
    }

    /// 逾期后缀：追加后可被剥离；已带后缀判定幂等。
    #[test]
    fn overdue_suffix_strip_and_has() {
        let raw = "背单词";
        assert!(!has_overdue_suffix(raw));
        let suffixed = format!("{raw}——2026年9月3日，任务逾期");
        assert!(has_overdue_suffix(&suffixed));
        assert_eq!(strip_overdue_suffix(&suffixed), raw);
        // 非逾期结尾不误伤
        assert_eq!(strip_overdue_suffix("背单词——2026年9月3日"), "背单词——2026年9月3日");
        assert_eq!(strip_overdue_suffix("——2026-09-03，任务逾期"), "——2026-09-03，任务逾期");
    }
}
