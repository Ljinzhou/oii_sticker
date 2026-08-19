//! 业务命令层（纯函数封装 repo，供 `#[tauri::command]` 包装）。
//!
//! 本模块只做"模型 ←→ repo"的业务编排（默认值、级联、文本变换），
//! 不直接接触 Tauri 运行时；阶段 3 起在其上薄包一层 `#[tauri::command]`。

use anyhow::Result;
use rusqlite::Connection;

use crate::db::{config_repo, prefs_repo, sticker_repo, todo_block_repo, todo_repo};
use crate::editing;
use crate::models::{EffectivePrefs, Sticker, StickerAttrs, StickerPrefs, SystemConfig, TodoBlock, TodoPatch};

// ── 便签 CRUD ──

/// 新建便签，返回自增 id。
pub fn create_sticker(conn: &Connection, new: &sticker_repo::NewSticker) -> Result<i64> {
    sticker_repo::insert(conn, new)
}

/// 按 id 读取便签。
pub fn get_sticker(conn: &Connection, id: i64) -> Result<Option<Sticker>> {
    sticker_repo::get(conn, id)
}

/// 列出全部便签。
pub fn list_stickers(conn: &Connection) -> Result<Vec<Sticker>> {
    sticker_repo::list_all(conn)
}

/// 部分更新便签。
pub fn update_sticker(
    conn: &Connection,
    id: i64,
    patch: &sticker_repo::StickerPatch,
) -> Result<()> {
    sticker_repo::update(conn, id, patch)
}

/// 删除便签（依赖外键级联清理）。
pub fn delete_sticker(conn: &Connection, id: i64) -> Result<()> {
    sticker_repo::delete(conn, id)
}

// ── 提醒 ──

/// 写入/覆盖便签提醒属性。
pub fn set_reminder(conn: &Connection, attrs: &StickerAttrs) -> Result<()> {
    sticker_repo::upsert_attrs(conn, attrs)
}

/// 清除便签提醒属性（写空字段，保留行）。
pub fn clear_reminder(conn: &Connection, sticker_id: i64) -> Result<()> {
    sticker_repo::upsert_attrs(
        conn,
        &StickerAttrs {
            sticker_id,
            ..Default::default()
        },
    )
}

/// 读取便签提醒属性。
pub fn get_reminder(conn: &Connection, sticker_id: i64) -> Result<Option<StickerAttrs>> {
    sticker_repo::get_attrs(conn, sticker_id)
}

// ── 偏好 ──

/// 写入/覆盖便签偏好。
pub fn update_sticker_prefs(conn: &Connection, prefs: &StickerPrefs) -> Result<()> {
    prefs_repo::upsert(conn, prefs)
}

/// 重置便签偏好（删除该行，恢复系统默认）。
pub fn reset_sticker_prefs(conn: &Connection, sticker_id: i64) -> Result<()> {
    prefs_repo::delete(conn, sticker_id)
}

/// 计算便签最终生效偏好（prefs → sticker.bg_color → system → 兜底）。
pub fn effective_prefs(
    conn: &Connection,
    config: &SystemConfig,
    sticker_id: i64,
) -> Result<EffectivePrefs> {
    let prefs = prefs_repo::get(conn, sticker_id)?.unwrap_or_default();
    let sticker_bg = sticker_repo::get(conn, sticker_id)?.and_then(|s| s.bg_color);
    Ok(config.effective(&prefs, sticker_bg.as_deref()))
}

// ── 配置 ──

/// 读取全部配置（返回 SystemConfig 快照）。
pub fn get_config(conn: &Connection) -> Result<SystemConfig> {
    config_repo::load_all(conn)
}

/// 写入单条配置。
pub fn set_config(conn: &Connection, key: &str, value: &str) -> Result<()> {
    config_repo::set(conn, key, value)
}

// ── 待办 ──

/// 翻转 markdown 内容第 `line` 行的 todo 状态并返回新内容。
pub fn toggle_todo(content: &str, line: usize) -> Option<String> {
    editing::toggle_todo_in_content(content, line)
}

/// 翻转便签内 todo 并落库（读取 → 文本变换 → 写回）。
pub fn toggle_todo_in_sticker(conn: &Connection, sticker_id: i64, line: usize) -> Result<bool> {
    let Some(sticker) = sticker_repo::get(conn, sticker_id)? else {
        return Ok(false);
    };
    let Some(new_content) = editing::toggle_todo_in_content(&sticker.content, line) else {
        return Ok(false);
    };
    sticker_repo::update(
        conn,
        sticker_id,
        &sticker_repo::StickerPatch {
            content: Some(new_content),
            ..Default::default()
        },
    )?;
    Ok(true)
}

/// 列出某个便签的所有 todo。
pub fn list_todos(conn: &Connection, sticker_id: i64) -> Result<Vec<crate::models::TodoItem>> {
    todo_repo::list_by_sticker(conn, sticker_id)
}

// ── 独立 Todo 块 ──

pub fn list_todo_blocks(conn: &Connection, sticker_id: i64) -> Result<Vec<TodoBlock>> {
    todo_block_repo::list_by_sticker(conn, sticker_id)
}

pub fn get_todo_block(conn: &Connection, id: &str) -> Result<Option<TodoBlock>> {
    todo_block_repo::get(conn, id)
}

pub fn create_todo_block(conn: &Connection, sticker_id: i64, parent_id: Option<&str>) -> Result<TodoBlock> {
    todo_block_repo::create(conn, sticker_id, parent_id)
}

pub fn update_todo_block(conn: &Connection, id: &str, patch: &TodoPatch) -> Result<TodoBlock> {
    todo_block_repo::update(conn, id, patch)
}

pub fn delete_todo_block(conn: &Connection, id: &str) -> Result<Option<i64>> {
    todo_block_repo::delete(conn, id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        schema::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn sticker_lifecycle_and_reminder() {
        let conn = test_conn();
        let id = create_sticker(
            &conn,
            &sticker_repo::NewSticker {
                title: "业务层便签".into(),
                content: "- [ ] 任务一\n- [ ] 任务二".into(),
                ..Default::default()
            },
        )
        .unwrap();

        // 提醒写入/读取/清除
        set_reminder(
            &conn,
            &StickerAttrs {
                sticker_id: id,
                remind_at: Some("2026-12-25 08:00:00".into()),
                remind_rule: Some("yearly:12-25".into()),
                is_recurring: true,
                ..Default::default()
            },
        )
        .unwrap();
        let attrs = get_reminder(&conn, id).unwrap().unwrap();
        assert_eq!(attrs.remind_rule.as_deref(), Some("yearly:12-25"));
        clear_reminder(&conn, id).unwrap();
        assert!(get_reminder(&conn, id).unwrap().unwrap().remind_at.is_none());

        // todo 翻转并落库
        assert!(toggle_todo_in_sticker(&conn, id, 0).unwrap());
        let s = get_sticker(&conn, id).unwrap().unwrap();
        assert!(s.content.starts_with("- [x] 任务一"), "got: {}", s.content);
        assert!(!toggle_todo_in_sticker(&conn, id, 99).unwrap());

        // 偏好与 effective
        update_sticker_prefs(
            &conn,
            &StickerPrefs {
                sticker_id: id,
                opacity: Some(0.66),
                ..Default::default()
            },
        )
        .unwrap();
        let cfg = get_config(&conn).unwrap();
        let eff = effective_prefs(&conn, &cfg, id).unwrap();
        assert_eq!(eff.opacity, 0.66);

        delete_sticker(&conn, id).unwrap();
        assert!(get_sticker(&conn, id).unwrap().is_none());
    }
}
