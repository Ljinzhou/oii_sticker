//! 工作空间文件系统的外部改动同步。
//!
//! 便签正文以 `stickers/<id>-<标题>.md` 为主存储，title/content 在 DB 里有回退副本。
//! 用户可能直接用编辑器/资源管理器增删改这些文件，程序必须跟上：
//!
//! - 新增 `7-想法.md` → 导入为便签（沿用文件名里的 id）；
//! - 新增 `随手记.md`（没有 id 前缀）→ 导入后重命名文件为 `{新id}-随手记.md`；
//! - 删除文件 → 删除对应便签（连带 assets/<id>/ 清理）；
//! - 改内容 → 同步 DB 回退副本（**不回写文件**，文件始终是主存储）；
//! - 改文件名里的标题 → 同步 DB title（**不重命名文件**，用户改名即意图）。
//!
//! `sync_once` 是幂等的纯 DB+文件操作（不依赖 watcher），由后台线程在目录快照变化时调用，
//! 因此切换工作空间、程序重启、漏事件都能自愈；watcher 只是为了"快"。

use anyhow::Result;
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

use super::layout::{self, Layout};
use super::md_store;
use crate::commands;
use crate::db::sticker_repo::{self, NewSticker, StickerPatch};

/// 一次同步的变更集合（供事件推送）。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SyncReport {
    /// 外部新增并导入的便签 id。
    pub imported: Vec<i64>,
    /// 外部修改（内容或标题）后同步到 DB 的便签 id。
    pub updated: Vec<i64>,
    /// 文件被外部删除而移除的便签 id。
    pub removed: Vec<i64>,
}

impl SyncReport {
    pub fn is_empty(&self) -> bool {
        self.imported.is_empty() && self.updated.is_empty() && self.removed.is_empty()
    }

    /// 所有受影响的便签 id（去重，供前端刷新）。
    pub fn touched(&self) -> Vec<i64> {
        let mut ids: Vec<i64> = self
            .imported
            .iter()
            .chain(self.updated.iter())
            .chain(self.removed.iter())
            .copied()
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }
}

/// 目录快照项：文件名 + 修改时间（秒）+ 大小；用于判断"是否值得跑一次同步"。
pub type Snapshot = Vec<(String, u64, u64)>;

/// 扫描 stickers/ 得到轻量快照（目录不存在时为空）。
pub fn snapshot(db_path: &str) -> Snapshot {
    let dir = Layout::at(&commands::ws_root(db_path)).stickers_dir();
    let mut out: Snapshot = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        out.push((entry.file_name().to_string_lossy().into_owned(), mtime, meta.len()));
    }
    out.sort();
    out
}

/// 文件名解析：`12-标题.md` → (Some(12), "标题")；无 id 前缀 → (None, "文件名"）。
fn parse_sticker_file(file_name: &str) -> Option<(Option<i64>, String)> {
    let stem = file_name.strip_suffix(".md")?;
    let stem = stem.trim();
    if stem.is_empty() {
        return None;
    }
    match stem.split_once('-') {
        Some((head, rest)) if !rest.trim().is_empty() && head.chars().all(|c| c.is_ascii_digit()) && !head.is_empty() => {
            let id = head.parse::<i64>().ok()?;
            Some((Some(id), rest.trim().to_string()))
        }
        _ => Some((None, stem.to_string())),
    }
}

/// 让 DB 与 `stickers/` 目录一致（幂等）。
pub fn sync_once(conn: &Connection, db_path: &str) -> Result<SyncReport> {
    let root = commands::ws_root(db_path);
    let dir = Layout::at(&root).stickers_dir();
    let mut report = SyncReport::default();
    // 目录不存在（被移动/卸载）时不动 DB：否则会把"整个目录读不到"误判成"便签全被删了"
    if !dir.is_dir() {
        return Ok(report);
    }

    // 1) 磁盘扫描：按 id 归并（同 id 有多个文件时取文件名排序后的第一个）
    let mut by_id: Vec<(i64, String, String, PathBuf)> = Vec::new(); // (id, title, file_name, path)
    let mut without_id: Vec<(String, String, PathBuf)> = Vec::new(); // (title, file_name, path)
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    entries.sort();
    for path in entries {
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Some((id, title)) = parse_sticker_file(&file_name) else {
            continue; // 非 .md / 空名：不参与同步
        };
        match id {
            Some(id) => {
                if by_id.iter().any(|(existing, ..)| *existing == id) {
                    continue; // 同 id 多文件：保留先出现的一个，其余忽略（不误删）
                }
                by_id.push((id, title, file_name, path));
            }
            None => without_id.push((title, file_name, path)),
        }
    }

    // 2) DB 现有便签
    let existing = sticker_repo::list_all(conn)?;
    let mut seen_ids: Vec<i64> = Vec::new();

    // 3) 已有 id：内容 / 标题变化 → 回写 DB 副本（不碰文件）
    for (id, title, _file_name, path) in &by_id {
        let Some(current) = existing.iter().find(|s| s.id == *id) else {
            // DB 里没有 → 导入（沿用文件名里的 id）
            let content = std::fs::read_to_string(path).unwrap_or_default();
            insert_with_id(conn, *id, title, &content)?;
            report.imported.push(*id);
            continue;
        };
        seen_ids.push(*id);
        let content = std::fs::read_to_string(path).unwrap_or_default();
        let mut patch = StickerPatch::default();
        if content != current.content {
            patch.content = Some(content);
        }
        if *title != current.title {
            patch.title = Some(title.clone());
        }
        if patch.content.is_none() && patch.title.is_none() {
            continue; // 一致：无动作（幂等）
        }
        sticker_repo::update(conn, *id, &patch)?;
        if let Some(content) = &patch.content {
            // 内容变化也要清掉"标签已无"的孤儿任务，避免 Todo 窗口继续显示
            let _ = commands::collect_orphan_blocks(conn, *id, content);
        }
        report.updated.push(*id);
    }

    // 4) 无 id 前缀的文件：导入（自增 id）后把文件改成规范名
    for (title, file_name, path) in &without_id {
        let content = std::fs::read_to_string(path).unwrap_or_default();
        let new = NewSticker {
            title: title.clone(),
            content,
            ..Default::default()
        };
        let id = sticker_repo::insert(conn, &new)?;
        let renamed = layout::sticker_file_name(id, title);
        let _ = std::fs::rename(path, dir.join(&renamed));
        tracing::info!("[sync] 导入外部文件 {file_name} → {renamed}（id={id}）");
        report.imported.push(id);
    }

    // 5) DB 有、磁盘没有 → 便签被外部删除（含资产目录清理）
    for sticker in &existing {
        if seen_ids.contains(&sticker.id) {
            continue;
        }
        commands::delete_sticker(conn, sticker.id, db_path)?;
        report.removed.push(sticker.id);
    }

    Ok(report)
}

/// 指定 id 插入（导入外部文件时沿用文件名里的 id）。
fn insert_with_id(conn: &Connection, id: i64, title: &str, content: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO stickers
           (id, title, content, heading_level, pos_x, pos_y, width, height,
            opacity, always_on_top, auto_scroll, is_completed, display_mode)
         VALUES (?1, ?2, ?3, 0, 120, 120, 400, 500, 0.9, 0, 0, 0, 'display')",
        params![id, title, content],
    )?;
    Ok(())
}

/// 后台线程：目录快照变化 → 同步一次 → 推送刷新事件。
/// 首次进入循环先跑一次（程序启动 / 切换工作空间后立即对齐）。
pub fn spawn(app: tauri::AppHandle, state: crate::state::AppState) {
    tracing::info!("[同步] 启动工作空间文件同步线程（快照轮询）");
    let _ = std::thread::Builder::new()
        .name("workspace-sync".into())
        .spawn(move || run_loop(app, state));
}

const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(1200);

fn run_loop(app: tauri::AppHandle, state: crate::state::AppState) {
    let mut last: Option<(String, Snapshot)> = None;
    loop {
        let db_path = state.db_path();
        if db_path.is_empty() {
            std::thread::sleep(POLL_INTERVAL);
            continue;
        }
        let snap = snapshot(&db_path);
        let changed = match &last {
            Some((path, prev)) => path != &db_path || prev != &snap,
            None => true,
        };
        if changed {
            last = Some((db_path.clone(), snap));
            match state.with_conn(|conn| sync_once(conn, &db_path)) {
                Ok(report) => {
                    if !report.is_empty() {
                        tracing::info!(
                            "[同步] 外部改动：新增 {:?} / 更新 {:?} / 删除 {:?}",
                            report.imported,
                            report.updated,
                            report.removed
                        );
                        for id in report.touched() {
                            crate::events::emit_push_update(&app, id);
                        }
                        // 被外部删除的便签：把它的窗口一并关掉
                        // （前端拿到空数据只会变成空白窗，留在桌面上反而困惑）
                        {
                            use tauri::Manager;
                            for id in &report.removed {
                                if let Some(win) = app.get_webview_window(&format!("sticker-{id}")) {
                                    let _ = win.close();
                                    tracing::info!("[同步] 便签 {id} 的文件已不存在，关闭其窗口");
                                }
                            }
                        }
                    }
                }
                Err(e) => tracing::warn!("[同步] 失败：{e}"),
            }
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// 立即同步一次（命令层用：切换工作空间后、手动刷新）。
pub fn sync_now(state: &crate::state::AppState) -> Result<SyncReport> {
    let db_path = state.db_path();
    if db_path.is_empty() {
        return Ok(SyncReport::default());
    }
    state.with_conn(|conn| sync_once(conn, &db_path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::connection::open;

    struct Env {
        dir: PathBuf,
        db_path: String,
        conn: Connection,
        layout: Layout,
    }

    impl Drop for Env {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn env(tag: &str) -> Env {
        let dir = std::env::temp_dir().join(format!("ws-sync-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let layout = Layout::at(&dir.join("ws"));
        layout::ensure_layout(&layout, "同步测试").unwrap();
        let db_path = layout.db_path().to_string_lossy().into_owned();
        let conn = open(&layout.db_path()).unwrap();
        crate::db::schema::run_migrations(&conn).unwrap();
        Env { dir, db_path, conn, layout }
    }

    fn write(layout: &Layout, name: &str, content: &str) {
        std::fs::write(layout.stickers_dir().join(name), content).unwrap();
    }

    #[test]
    fn imports_file_with_id_from_name() {
        let e = env("import-id");
        write(&e.layout, "7-外来想法.md", "# 外部写的");
        let report = sync_once(&e.conn, &e.db_path).unwrap();
        assert_eq!(report.imported, vec![7]);
        let s = sticker_repo::get(&e.conn, 7).unwrap().expect("应导入 id=7");
        assert_eq!(s.title, "外来想法");
        assert_eq!(s.content, "# 外部写的");
        // 幂等：再同步一次无变化
        let again = sync_once(&e.conn, &e.db_path).unwrap();
        assert!(again.is_empty(), "第二次应无变更：{again:?}");
    }

    #[test]
    fn imports_file_without_id_and_renames_it() {
        let e = env("import-plain");
        write(&e.layout, "随手记.md", "临时内容");
        let report = sync_once(&e.conn, &e.db_path).unwrap();
        assert_eq!(report.imported.len(), 1);
        let id = report.imported[0];
        let s = sticker_repo::get(&e.conn, id).unwrap().unwrap();
        assert_eq!(s.title, "随手记");
        assert_eq!(s.content, "临时内容");
        // 文件被改成规范名（{id}-标题.md），原文件不再存在
        assert!(!e.layout.stickers_dir().join("随手记.md").exists());
        assert!(e.layout.stickers_dir().join(format!("{id}-随手记.md")).is_file());
    }

    #[test]
    fn external_content_edit_updates_db_copy_both_ways_consistent() {
        let e = env("content");
        write(&e.layout, "3-笔记.md", "旧内容");
        sync_once(&e.conn, &e.db_path).unwrap();
        // 外部编辑（文件是主存储）
        write(&e.layout, "3-笔记.md", "外部改过的内容");
        let report = sync_once(&e.conn, &e.db_path).unwrap();
        assert_eq!(report.updated, vec![3]);
        assert_eq!(sticker_repo::get(&e.conn, 3).unwrap().unwrap().content, "外部改过的内容");
        // 程序没有回写文件
        assert_eq!(
            std::fs::read_to_string(e.layout.stickers_dir().join("3-笔记.md")).unwrap(),
            "外部改过的内容"
        );
    }

    #[test]
    fn external_rename_updates_title_without_renaming_file() {
        let e = env("rename");
        write(&e.layout, "5-旧标题.md", "内容");
        sync_once(&e.conn, &e.db_path).unwrap();
        // 用户在资源管理器里改名（同一 id）
        std::fs::rename(
            e.layout.stickers_dir().join("5-旧标题.md"),
            e.layout.stickers_dir().join("5-新标题.md"),
        )
        .unwrap();
        let report = sync_once(&e.conn, &e.db_path).unwrap();
        assert_eq!(report.updated, vec![5]);
        assert_eq!(sticker_repo::get(&e.conn, 5).unwrap().unwrap().title, "新标题");
        // 文件保持用户给的名称
        assert!(e.layout.stickers_dir().join("5-新标题.md").is_file());
    }

    #[test]
    fn deleted_file_removes_sticker_and_assets() {
        let e = env("delete");
        write(&e.layout, "9-待删.md", "内容");
        let assets = e.layout.assets_dir().join("9");
        std::fs::create_dir_all(&assets).unwrap();
        std::fs::write(assets.join("img.png"), b"png").unwrap();
        sync_once(&e.conn, &e.db_path).unwrap();
        assert!(sticker_repo::get(&e.conn, 9).unwrap().is_some());

        std::fs::remove_file(e.layout.stickers_dir().join("9-待删.md")).unwrap();
        let report = sync_once(&e.conn, &e.db_path).unwrap();
        assert_eq!(report.removed, vec![9]);
        assert!(sticker_repo::get(&e.conn, 9).unwrap().is_none(), "便签应被删除");
        assert!(!assets.exists(), "对应 assets/<id>/ 应一并清理");
    }

    /// 目录整个不可读（被移走/卸载）时绝不能把 DB 便签当作"被删了"。
    #[test]
    fn missing_dir_does_not_delete_anything() {
        let e = env("missing-dir");
        write(&e.layout, "1-保留.md", "内容");
        sync_once(&e.conn, &e.db_path).unwrap();
        std::fs::remove_dir_all(e.layout.stickers_dir()).unwrap();

        let report = sync_once(&e.conn, &e.db_path).unwrap();
        assert!(report.is_empty(), "不应产生任何变更：{report:?}");
        assert!(sticker_repo::get(&e.conn, 1).unwrap().is_some(), "便签必须保留");
    }

    #[test]
    fn non_markdown_files_are_ignored() {
        let e = env("other-files");
        std::fs::write(e.layout.stickers_dir().join("notes.txt"), b"ignore me").unwrap();
        std::fs::create_dir_all(e.layout.stickers_dir().join("sub")).unwrap();
        let report = sync_once(&e.conn, &e.db_path).unwrap();
        assert!(report.is_empty());
        assert!(sticker_repo::list_all(&e.conn).unwrap().is_empty());
        assert!(e.layout.stickers_dir().join("notes.txt").is_file(), "非 md 文件不动");
    }

    /// 程序自身写文件（内部保存）不应被同步误判成外部改动。
    #[test]
    fn internal_save_is_not_reported_as_change() {
        let e = env("internal");
        let id = commands::create_sticker(
            &e.conn,
            &NewSticker { title: "内部".into(), content: "正文".into(), ..Default::default() },
            &e.db_path,
        )
        .unwrap();
        let report = sync_once(&e.conn, &e.db_path).unwrap();
        assert!(report.is_empty(), "自己写的文件不该产生同步变更：{report:?}");
        let _ = id;
    }

    #[test]
    fn snapshot_detects_file_changes() {
        let e = env("snap");
        assert!(snapshot(&e.db_path).is_empty());
        write(&e.layout, "1-a.md", "x");
        let first = snapshot(&e.db_path);
        assert_eq!(first.len(), 1);
        write(&e.layout, "2-b.md", "y");
        assert_ne!(snapshot(&e.db_path), first, "新增文件后快照应不同");
    }

    #[test]
    fn report_touched_dedups_and_sorts() {
        let report = SyncReport {
            imported: vec![3, 1],
            updated: vec![1],
            removed: vec![9],
        };
        assert_eq!(report.touched(), vec![1, 3, 9]);
        assert!(!report.is_empty());
        assert!(SyncReport::default().is_empty());
    }

    #[test]
    fn parse_sticker_file_variants() {
        assert_eq!(parse_sticker_file("12-标题.md"), Some((Some(12), "标题".into())));
        assert_eq!(parse_sticker_file("随手记.md"), Some((None, "随手记".into())));
        assert_eq!(parse_sticker_file("无扩展"), None);
        assert_eq!(parse_sticker_file("3-.md"), Some((None, "3-".into())));
    }
}
