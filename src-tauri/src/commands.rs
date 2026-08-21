//! 业务命令层（纯函数封装 repo，供 `#[tauri::command]` 包装）。
//!
//! 本模块只做"模型 ←→ repo"的业务编排（默认值、级联、文本变换），
//! 不直接接触 Tauri 运行时；阶段 3 起在其上薄包一层 `#[tauri::command]`。

use anyhow::{bail, Result};
use rusqlite::Connection;

use crate::db::{config_repo, prefs_repo, sticker_repo, todo_block_repo, todo_repo};
use crate::editing;
use crate::models::{EffectivePrefs, Sticker, StickerAttrs, StickerPrefs, SystemConfig, TodoBlock, TodoPatch};
use crate::workspace::{layout, md_store};

// ── 便签 CRUD ──

/// 当前工作控件根目录（从当前 DB 路径推算）。
pub fn ws_root(conn_path: &str) -> std::path::PathBuf {
    layout::root_from_db_path(std::path::Path::new(conn_path)).unwrap_or_default()
}

/// 新建便签，返回自增 id；md 落盘（主存储），DB content 列保留作回退。
pub fn create_sticker(conn: &Connection, new: &sticker_repo::NewSticker, db_path: &str) -> Result<i64> {
    let id = sticker_repo::insert(conn, new)?;
    let root = ws_root(db_path);
    let file_name = layout::sticker_file_name(id, &new.title);
    md_store::write(&root, &file_name, &new.content)?;
    Ok(id)
}

/// 按 id 读取便签；md 内容优先（md 缺失回退 DB content 列）。
pub fn get_sticker(conn: &Connection, id: i64, db_path: &str) -> Result<Option<Sticker>> {
    let Some(mut sticker) = sticker_repo::get(conn, id)? else {
        return Ok(None);
    };
    let root = ws_root(db_path);
    let file_name = layout::sticker_file_name(sticker.id, &sticker.title);
    if let Some(content) = md_store::load(&root, &file_name)? {
        sticker.content = content;
    }
    Ok(Some(sticker))
}

/// 列出全部便签。
pub fn list_stickers(conn: &Connection) -> Result<Vec<Sticker>> {
    sticker_repo::list_all(conn)
}

/// 部分更新便签；标题变化同步重命名 md，content 原子写 md（主存储）后双写 DB 列（回退）。
pub fn update_sticker(
    conn: &Connection,
    id: i64,
    patch: &sticker_repo::StickerPatch,
    db_path: &str,
) -> Result<()> {
    let root = ws_root(db_path);
    if let Some(current) = sticker_repo::get(conn, id)? {
        // 标题变化：用旧标题推旧名，重命名 md → {id}-新标题.md
        if let Some(new_title) = &patch.title {
            let old = layout::sticker_file_name(current.id, &current.title);
            let _ = md_store::rename_for_title(&root, &old, current.id, new_title)?;
        }
        // content patch → 原子写 md（主存储），再双写 DB（回退）。
        // 文件名一律按「生效标题」派生：标题同时变化时用新标题，避免写回旧文件名。
        if let Some(content) = &patch.content {
            let title = patch.title.as_deref().unwrap_or(&current.title);
            let file_name = layout::sticker_file_name(current.id, title);
            md_store::write(&root, &file_name, content)?;
        }
    }
    sticker_repo::update(conn, id, patch)
}

/// 删除便签（依赖外键级联清理），md 与资产目录一并移除。
pub fn delete_sticker(conn: &Connection, id: i64, db_path: &str) -> Result<()> {
    if let Some(sticker) = sticker_repo::get(conn, id)? {
        let root = ws_root(db_path);
        let file_name = layout::sticker_file_name(id, &sticker.title);
        let _ = md_store::remove(&root, &file_name);
        let asset_dir = root.join("assets").join(id.to_string());
        let _ = std::fs::remove_dir_all(asset_dir);
    }
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

/// 翻转便签内 todo 并落库（读取 md 主存储 → 文本变换 → 原子写 md + 双写 DB）。
pub fn toggle_todo_in_sticker(conn: &Connection, sticker_id: i64, line: usize, db_path: &str) -> Result<bool> {
    let Some(sticker) = get_sticker(conn, sticker_id, db_path)? else {
        return Ok(false);
    };
    let Some(new_content) = editing::toggle_todo_in_content(&sticker.content, line) else {
        return Ok(false);
    };
    update_sticker(
        conn,
        sticker_id,
        &sticker_repo::StickerPatch {
            content: Some(new_content),
            ..Default::default()
        },
        db_path,
    )?;
    Ok(true)
}

/// 列出某个便签的所有 todo。
pub fn list_todos(conn: &Connection, sticker_id: i64) -> Result<Vec<crate::models::TodoItem>> {
    todo_repo::list_by_sticker(conn, sticker_id)
}

// ── 独立 Todo 块 ──

pub fn list_todo_blocks(conn: &Connection, sticker_id: i64) -> Result<(Vec<TodoBlock>, bool)> {
    // 列表前先为无标记的根任务补写标记（幂等），保证块与正文一致；
    // 返回是否补写了标记（调用方据此通知便签窗口刷新正文）。
    let retagged = !todo_block_repo::retag_orphans_for_sticker(conn, sticker_id)?.is_empty();
    Ok((todo_block_repo::list_by_sticker(conn, sticker_id)?, retagged))
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

/// 把 `<todo-block>` 标记追加到便签内容末尾（幂等；已含标记则跳过）。
/// 供 Todo 窗口新建根任务后同步标记，保证块与正文一致、不产生孤儿。
pub fn sync_todo_marker(conn: &Connection, sticker_id: i64, id: &str, db_path: &str) -> Result<bool> {
    let Some(sticker) = get_sticker(conn, sticker_id, db_path)? else {
        bail!("便签不存在");
    };
    let tagged = todo_block_repo::tagged_ids(&sticker.content);
    if tagged.iter().any(|t| t == id) {
        return Ok(false);
    }
    let tag = format!("<todo-block id=\"{id}\"></todo-block>");
    let next = if sticker.content.trim().is_empty() {
        tag
    } else {
        format!("{}\n\n{tag}", sticker.content.trim_end())
    };
    update_sticker(conn, sticker_id, &sticker_repo::StickerPatch {
        content: Some(next),
        ..Default::default()
    }, db_path)?;
    Ok(true)
}

/// 全局补写孤儿 Todo 块标记（启动时调用一次），返回每便签被补写的 id 列表供广播。
pub fn retag_orphan_todos(conn: &Connection) -> Result<Vec<(i64, Vec<String>)>> {
    todo_block_repo::retag_all_orphans(conn)
}

/// 重排任务（拖拽排序），返回所属便签 id（空列表返回 None）。
pub fn reorder_todo(conn: &Connection, ids: &[String]) -> Result<Option<i64>> {
    if ids.is_empty() {
        return Ok(None);
    }
    let sticker_id = todo_block_repo::get(conn, &ids[0])?.map(|b| b.sticker_id);
    todo_block_repo::reorder(conn, ids)?;
    Ok(sticker_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema;
    use std::path::PathBuf;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        schema::run_migrations(&conn).unwrap();
        conn
    }

    /// 临时工作控件根目录：ensure_layout 后返回 (root, db_path)。
    fn tmp_ws(tag: &str) -> (PathBuf, String) {
        let d = std::env::temp_dir().join(format!("ws-cmd-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        crate::workspace::layout::ensure_layout(&crate::workspace::layout::Layout::at(&d), "t")
            .unwrap();
        let db = d.join("data").join("index.db").to_string_lossy().into_owned();
        (d, db)
    }

    #[test]
    fn sticker_lifecycle_and_reminder() {
        let conn = test_conn();
        let (root, db_path) = tmp_ws("life");
        let id = create_sticker(
            &conn,
            &sticker_repo::NewSticker {
                title: "业务层便签".into(),
                content: "- [ ] 任务一\n- [ ] 任务二".into(),
                ..Default::default()
            },
            &db_path,
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
        assert!(toggle_todo_in_sticker(&conn, id, 0, &db_path).unwrap());
        let s = get_sticker(&conn, id, &db_path).unwrap().unwrap();
        assert!(s.content.starts_with("- [x] 任务一"), "got: {}", s.content);
        assert!(!toggle_todo_in_sticker(&conn, id, 99, &db_path).unwrap());

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

        delete_sticker(&conn, id, &db_path).unwrap();
        assert!(get_sticker(&conn, id, &db_path).unwrap().is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sticker_md_is_primary_storage() {
        let conn = test_conn();
        let (root, db_path) = tmp_ws("md");
        let id = create_sticker(
            &conn,
            &sticker_repo::NewSticker {
                title: "主存储".into(),
                content: "# 正文A".into(),
                ..Default::default()
            },
            &db_path,
        )
        .unwrap();
        let file = layout::sticker_file_name(id, "主存储");
        assert!(root.join("stickers").join(&file).exists(), "create 后 md 文件缺失");
        assert_eq!(
            md_store::load(&root, &file).unwrap().unwrap(),
            "# 正文A"
        );

        // content 更新 → md 同步写入（主存储）
        let new_content = "# 正文B".to_string();
        update_sticker(
            &conn,
            id,
            &sticker_repo::StickerPatch { content: Some(new_content.clone()), ..Default::default() },
            &db_path,
        )
        .unwrap();
        assert_eq!(md_store::load(&root, &file).unwrap().unwrap(), "# 正文B");

        // 标题变更 → md 文件重命名
        update_sticker(
            &conn,
            id,
            &sticker_repo::StickerPatch { title: Some("新标题".into()), ..Default::default() },
            &db_path,
        )
        .unwrap();
        assert!(!root.join("stickers").join(&file).exists(), "旧文件未移除");
        let new_file = layout::sticker_file_name(id, "新标题");
        assert!(root.join("stickers").join(&new_file).exists(), "新文件未生成");
        assert_eq!(md_store::load(&root, &new_file).unwrap().unwrap(), "# 正文B");

        // get 以 md 为准
        let s = get_sticker(&conn, id, &db_path).unwrap().unwrap();
        assert_eq!(s.title, "新标题");
        assert_eq!(s.content, "# 正文B");

        // 删除 → md 一并移除
        delete_sticker(&conn, id, &db_path).unwrap();
        assert!(!root.join("stickers").join(&new_file).exists());
        assert!(get_sticker(&conn, id, &db_path).unwrap().is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn update_with_title_and_content_writes_to_new_file_and_db() {
        let conn = test_conn();
        let (root, db_path) = tmp_ws("both");
        let id = create_sticker(
            &conn,
            &sticker_repo::NewSticker {
                title: "旧题".into(),
                content: "X".into(),
                ..Default::default()
            },
            &db_path,
        )
        .unwrap();

        // 组合补丁：标题+内容同时变更 → md 重命名到新标题文件且写入新内容
        update_sticker(
            &conn,
            id,
            &sticker_repo::StickerPatch {
                title: Some("新题".into()),
                content: Some("Y".into()),
                ..Default::default()
            },
            &db_path,
        )
        .unwrap();

        let old_f = layout::sticker_file_name(id, "旧题");
        let new_f = layout::sticker_file_name(id, "新题");
        assert!(!root.join("stickers").join(&old_f).exists(), "旧文件不应残留");
        assert_eq!(
            md_store::load(&root, &new_f).unwrap().unwrap(),
            "Y",
            "新文件名应为 Y"
        );

        // get 以 md 为准
        let s = get_sticker(&conn, id, &db_path).unwrap().unwrap();
        assert_eq!(s.title, "新题");
        assert_eq!(s.content, "Y");

        // DB content 列双写验证（回退保险）
        let row = sticker_repo::get(&conn, id).unwrap().unwrap();
        assert_eq!(row.content, "Y");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn get_sticker_falls_back_to_db_content_when_md_missing() {
        let conn = test_conn();
        let (root, db_path) = tmp_ws("fallback");
        let id = create_sticker(
            &conn,
            &sticker_repo::NewSticker {
                title: "回退".into(),
                content: "DB 内容".into(),
                ..Default::default()
            },
            &db_path,
        )
        .unwrap();

        let file = layout::sticker_file_name(id, "回退");
        let path = root.join("stickers").join(&file);
        assert!(path.exists());
        std::fs::remove_file(&path).unwrap();

        let s = get_sticker(&conn, id, &db_path).unwrap().unwrap();
        assert_eq!(s.content, "DB 内容", "md 缺失应回退 DB content 列");
        let _ = std::fs::remove_dir_all(&root);
    }
}
