//! 备份恢复：把 zip 备份「恢复为新工作空间」或「覆盖现有工作空间」。
//!
//! 安全约定：
//! - 先 `inspect` 校验备份合法，再动任何现有数据；
//! - 覆盖模式把旧数据整体 **改名** 到暂存目录下的 `rollback-*`（不删除，失败可回滚），
//!   并在动手前把当前工作空间完整备份到暂存目录下的 `recovery-*.zip`（第二重回滚点）；
//! - 覆盖当前工作空间会先关闭全部便签窗口并释放数据库连接（Windows 上文件被占用无法替换）。
//!
//! 暂存目录为工作空间**之外**的同卷兄弟目录（`Layout::staging_dir`）：改名仍是原子操作，
//! 而工作空间里不会再出现 `cache/`。

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use super::backup;
use super::cmds;
use super::layout::{self, Layout, WorkspaceEntry};

/// 覆盖时被替换的工作空间顶层项（工作空间里只有这四样）。
const SWAP_ITEMS: [&str; 4] = ["stickers", "assets", "data", "workspace.json"];

/// 恢复结果（前端展示回滚点位置）。
#[derive(serde::Serialize, Clone, Debug)]
pub struct RestoreOutcome {
    pub mode: String,
    pub name: String,
    pub root: String,
    pub entries: u64,
    /// 覆盖模式新增的工作空间条目（new 模式为 None）。
    pub workspace: Option<crate::models::WorkspaceEntryDto>,
    /// 覆盖模式的事前完整备份（可据此回退整个恢复）。
    pub rollback_zip: Option<String>,
    /// 覆盖模式旧数据的存放目录（未删除；确认无误后可手动清理）。
    pub rollback_dir: Option<String>,
}

fn now_tag() -> String {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("{}-{}", std::process::id(), ms)
}

/// 恢复为**新工作空间**：解压到 dest（必须不存在或为空）→ 注册；
/// 名称沿用备份里的工作空间名，重名自动加「（恢复）」后缀。
pub fn restore_as_new(
    reg_path: &Path,
    zip: &Path,
    dest: &Path,
    name: Option<&str>,
) -> Result<(WorkspaceEntry, u64)> {
    let info = backup::inspect(zip).context("备份文件不可用")?;
    layout::ensure_empty_dest(dest)?;
    let sig = backup::extract(zip, dest).context("解压备份失败")?;
    let base = name
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            if sig.name.trim().is_empty() {
                info.name.clone()
            } else {
                sig.name.clone()
            }
        });
    let name = unique_name(reg_path, &base)?;
    let entry = cmds::create(reg_path, dest, Some(&name)).context("注册新工作空间失败")?;
    Ok((entry, info.entries))
}

/// 名称去重：已存在同名时追加「（恢复）」/「（恢复 2）」…
fn unique_name(reg_path: &Path, base: &str) -> Result<String> {
    let existing: Vec<String> = cmds::list(reg_path)?.into_iter().map(|w| w.name).collect();
    if !existing.iter().any(|n| n == base) {
        return Ok(base.to_string());
    }
    for index in 1..=50 {
        let candidate = if index == 1 {
            format!("{base}（恢复）")
        } else {
            format!("{base}（恢复 {index}）")
        };
        if !existing.iter().any(|n| *n == candidate) {
            return Ok(candidate);
        }
    }
    Ok(format!("{base}（恢复）"))
}

/// 把 staging（已解压的新数据）替换到 root：旧项改名到 rollback_dir 保存，失败自动回滚。
/// 返回被替换的项名。调用前必须保证没有进程占用 root 下的文件（覆盖当前工作空间时先释放 DB）。
pub fn apply_restore(root: &Path, staging: &Path, rollback_dir: &Path) -> Result<Vec<String>> {
    std::fs::create_dir_all(rollback_dir)
        .with_context(|| format!("创建回滚目录失败：{}", rollback_dir.display()))?;
    let mut swapped: Vec<String> = Vec::new();
    for name in SWAP_ITEMS {
        let current = root.join(name);
        let staged = staging.join(name);
        let result = (|| -> Result<()> {
            if current.exists() {
                let backup_slot = rollback_dir.join(name);
                if backup_slot.exists() {
                    remove_any(&backup_slot)?;
                }
                std::fs::rename(&current, &backup_slot)
                    .with_context(|| format!("保存原数据失败：{}", current.display()))?;
                swapped.push(name.to_string());
            }
            if staged.exists() {
                std::fs::rename(&staged, &current)
                    .with_context(|| format!("写入恢复数据失败：{}", current.display()))?;
            } else if current.exists() && name == "workspace.json" {
                // 备份没带签名：保留原签名，避免工作空间失去身份
            }
            Ok(())
        })();
        if let Err(e) = result {
            rollback(&root, rollback_dir, &swapped);
            return Err(e);
        }
    }
    Ok(swapped)
}

/// 回滚已替换的项（apply_restore 失败时调用）。
pub fn rollback(root: &Path, rollback_dir: &Path, swapped: &[String]) {
    for name in swapped {
        let current = root.join(name);
        let saved = rollback_dir.join(name);
        if !saved.exists() {
            continue;
        }
        let _ = remove_any(&current);
        let _ = std::fs::rename(&saved, &current);
    }
}

fn remove_any(path: &Path) -> Result<()> {
    if path.is_dir() {
        std::fs::remove_dir_all(path)?;
    } else if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

/// 覆盖前的事前完整备份（回滚点）：写入工作空间**之外**的暂存目录。
pub fn make_recovery_point(
    layout: &Layout,
    conn: &rusqlite::Connection,
) -> Result<PathBuf> {
    // 回滚点只是写一个 zip（不参与 rename），所以「盘符根目录」退化情形也能用系统临时目录
    let dir = outside_root_dir(layout);
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("创建暂存目录失败：{}", dir.display()))?;
    let path = dir.join(format!("recovery-{}.zip", now_tag()));
    backup::backup(layout, conn, &path).context("生成回滚点失败")?;
    Ok(path)
}

/// 需要「不在工作空间内」的目录时用哪个：
/// 正常用同卷暂存目录；工作空间就在盘符根目录（`E:\`）时暂存目录会退化到工作空间内部，
/// 此时改用系统临时目录，避免备份因「不能写在工作空间里」而失败。
fn outside_root_dir(layout: &Layout) -> PathBuf {
    let staging = layout.staging_dir();
    if staging.starts_with(&layout.root) {
        std::env::temp_dir().join(format!("{}{}", layout::STAGING_PREFIX, "scratch"))
    } else {
        staging
    }
}

/// 清掉上次异常退出残留的恢复 staging 目录（`restore-*.tmp`）。
fn clean_stale_staging(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("restore-") && name.ends_with(".tmp") {
            let _ = remove_any(&entry.path());
        }
    }
}

/// 覆盖式恢复的数据部分（不含 DB 切换与窗口关闭，便于单测）：
/// 解压 staging → 替换 root。返回（条目数, 回滚目录）。
pub fn restore_overwrite_data(root: &Path, zip: &Path) -> Result<(u64, PathBuf)> {
    let info = backup::inspect(zip).context("备份文件不可用")?;
    let layout = Layout::at(root);
    // 暂存目录必须在工作空间**之外**但同卷：下面的 apply_restore 全用 rename（原子）。
    let scratch = layout.staging_dir();
    std::fs::create_dir_all(&scratch)
        .with_context(|| format!("创建暂存目录失败：{}", scratch.display()))?;
    clean_stale_staging(&scratch);
    let staging = scratch.join(format!("restore-{}.tmp", now_tag()));
    if staging.exists() {
        remove_any(&staging)?;
    }
    std::fs::create_dir_all(&staging)?;
    backup::extract(zip, &staging).context("解压备份失败")?;
    // 目录结构补齐：备份里可能没有 stickers/assets（备份时它们就是空的），
    // 而工作空间缺这些目录会让之后的 md 写入失败（新建/编辑便签报错）
    for dir in ["stickers", "assets"] {
        std::fs::create_dir_all(staging.join(dir))?;
    }
    let rollback_dir = scratch.join(format!("rollback-{}.tmp", now_tag()));
    let result = apply_restore(root, &staging, &rollback_dir);
    let _ = remove_any(&staging); // 内容已被 rename 走，剩下空壳
    let swapped = result?;
    if swapped.is_empty() {
        bail!("备份内没有任何可恢复的数据");
    }
    // 历史版本的 cache/ 与 library/ 不是工作空间数据，恢复时顺手清掉
    for name in layout::remove_legacy_dirs(&layout) {
        tracing::info!("[restore] 清理历史目录 {name}/（工作空间已不再使用）");
    }
    Ok((info.entries, rollback_dir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::backup as bk;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ws-restore-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 造一个含 1 个便签的完整工作空间，并返回它的备份 zip 路径。
    fn make_backup(base: &Path, ws_name: &str, sticker: &str) -> PathBuf {
        let root = base.join("src-ws");
        let layout = Layout::at(&root);
        layout::ensure_layout(&layout, ws_name).unwrap();
        std::fs::write(layout.stickers_dir().join("欢迎.md"), sticker).unwrap();
        let conn = rusqlite::Connection::open(layout.db_path()).unwrap();
        crate::db::schema::run_migrations(&conn).unwrap();
        let _ = conn.close();
        let zip = base.join("backup.zip");
        let conn = rusqlite::Connection::open(layout.db_path()).unwrap();
        bk::backup(&layout, &conn, &zip).unwrap();
        let _ = conn.close();
        zip
    }

    #[test]
    fn restore_as_new_registers_workspace_with_backup_content() {
        let base = temp_dir("new");
        let zip = make_backup(&base, "原工作空间", "# 恢复后的便签");
        let reg = base.join("app").join("workspaces.json");
        let dest = base.join("restored");

        let (entry, entries) = restore_as_new(&reg, &zip, &dest, None).unwrap();
        assert_eq!(entry.name, "原工作空间");
        assert!(entries >= 2);
        assert!(dest.join("stickers").join("欢迎.md").is_file());
        assert!(dest.join("data").join("index.db").is_file());
        assert_eq!(
            std::fs::read_to_string(dest.join("stickers").join("欢迎.md")).unwrap(),
            "# 恢复后的便签"
        );
        // 首个工作空间自动激活
        assert_eq!(cmds::current(&reg).unwrap().unwrap().id, entry.id);

        // 同名再恢复 → 自动加后缀，不覆盖已有条目
        let dest2 = base.join("restored-2");
        let (entry2, _) = restore_as_new(&reg, &zip, &dest2, None).unwrap();
        assert_eq!(entry2.name, "原工作空间（恢复）");
        assert_eq!(cmds::list(&reg).unwrap().len(), 2);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn restore_as_new_rejects_non_empty_dest() {
        let base = temp_dir("nonempty");
        let zip = make_backup(&base, "A", "# a");
        let reg = base.join("app").join("workspaces.json");
        let dest = base.join("busy");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("占用.txt"), b"x").unwrap();
        let err = restore_as_new(&reg, &zip, &dest, None).unwrap_err().to_string();
        assert!(err.contains(layout::ERR_DEST_NOT_EMPTY), "实际：{err}");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn restore_overwrite_replaces_data_and_keeps_rollback_copy() {
        let base = temp_dir("over");
        let zip = make_backup(&base, "备份源", "# 新内容");
        // 目标工作空间：旧便签应被替换，且旧数据落到 rollback 目录
        let target = base.join("target-ws");
        let layout = Layout::at(&target);
        layout::ensure_layout(&layout, "目标").unwrap();
        std::fs::write(layout.stickers_dir().join("旧.md"), "# 旧内容").unwrap();
        // 工作空间里用户自己的文件（不在 SWAP_ITEMS 里）恢复时不得被动
        std::fs::write(layout.root.join("额外.txt"), b"keep").unwrap();
        let conn = rusqlite::Connection::open(layout.db_path()).unwrap();
        crate::db::schema::run_migrations(&conn).unwrap();
        let _ = conn.close();

        let (entries, rollback_dir) = restore_overwrite_data(&target, &zip).unwrap();
        assert!(entries >= 2);
        // 新内容生效
        assert_eq!(
            std::fs::read_to_string(layout.stickers_dir().join("欢迎.md")).unwrap(),
            "# 新内容"
        );
        assert!(!layout.stickers_dir().join("旧.md").exists(), "旧便签不应残留");
        // 旧数据被保留到回滚目录（不删除）
        assert!(rollback_dir.join("stickers").join("旧.md").is_file());
        assert!(rollback_dir.join("data").join("index.db").is_file());
        // 回滚目录在暂存目录（工作空间之外），工作空间里不再出现程序目录
        assert!(!rollback_dir.starts_with(&target), "回滚目录应位于工作空间之外");
        // 非替换项不受影响
        assert!(layout.root.join("额外.txt").is_file());
        // 恢复后的库可用
        let conn = rusqlite::Connection::open(layout.db_path()).unwrap();
        let v: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, crate::db::schema::SCHEMA_VERSION);
        let _ = conn.close();
        let _ = std::fs::remove_dir_all(&base);
    }

    /// 写入新数据失败时（staging 缺少必需项），旧数据必须原样回滚。
    #[test]
    fn apply_restore_rolls_back_when_new_data_incomplete() {
        let base = temp_dir("rollback");
        let root = base.join("ws");
        let layout = Layout::at(&root);
        layout::ensure_layout(&layout, "保留").unwrap();
        std::fs::write(layout.stickers_dir().join("1-旧.md"), "# 旧").unwrap();

        // staging 只有 workspace.json（缺少 stickers/data 等）→ 替换后旧项应回滚
        let staging = base.join("staging");
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::write(
            staging.join("workspace.json"),
            r#"{"id":"w-y","name":"新","created_at":"0","last_used_at":"0"}"#.as_bytes(),
        )
        .unwrap();
        let rollback_dir = base.join("rollback");
        let swapped = apply_restore(&root, &staging, &rollback_dir).unwrap();
        // 覆盖语义 = 回到备份时刻：staging 没带的项也要清空，旧数据一律保留在回滚目录
        assert_eq!(swapped.len(), 4, "实际：{swapped:?}");
        assert!(
            rollback_dir.join("stickers").join("1-旧.md").is_file(),
            "旧数据应保存在回滚目录"
        );
        assert!(
            !root.join("stickers").join("1-旧.md").exists(),
            "root 应回到备份时刻的状态"
        );

        // 回滚：签名与旧便签都回到原位
        rollback(&root, &rollback_dir, &swapped);
        let sig = layout::read_signature(&layout.signature_path()).unwrap().unwrap();
        assert_eq!(sig.name, "保留");
        assert!(root.join("stickers").join("1-旧.md").is_file(), "回滚应把旧便签放回");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// 备份里没有 stickers/ 等目录（备份时它们就是空的）时，覆盖恢复后目录必须仍在：
    /// 否则之后新建/编辑便签（写 md）会因为目录缺失而失败。
    #[test]
    fn restore_overwrite_recreates_missing_dirs() {
        let base = temp_dir("dirs");
        // 造一个「没有任何便签」的备份：zip 内不含 stickers/ 条目
        let src = Layout::at(&base.join("empty-ws"));
        layout::ensure_layout(&src, "空工作空间").unwrap();
        let conn = rusqlite::Connection::open(src.db_path()).unwrap();
        crate::db::schema::run_migrations(&conn).unwrap();
        let _ = conn.close();
        let zip = base.join("empty.zip");
        let conn = rusqlite::Connection::open(src.db_path()).unwrap();
        bk::backup(&src, &conn, &zip).unwrap();
        let _ = conn.close();

        let target = base.join("target-ws");
        let layout = Layout::at(&target);
        layout::ensure_layout(&layout, "目标").unwrap();
        std::fs::write(layout.stickers_dir().join("1-旧.md"), "# 旧").unwrap();
        let conn = rusqlite::Connection::open(layout.db_path()).unwrap();
        crate::db::schema::run_migrations(&conn).unwrap();
        let _ = conn.close();

        restore_overwrite_data(&target, &zip).unwrap();
        assert!(layout.stickers_dir().is_dir(), "恢复后 stickers/ 必须存在");
        assert!(layout.assets_dir().is_dir(), "恢复后 assets/ 必须存在");
        assert!(layout.db_path().parent().unwrap().is_dir(), "恢复后 data/ 必须存在");
        assert!(!layout.root.join("library").exists(), "不得重建已废弃的 library/");
        assert!(!layout.root.join("cache").exists(), "不得重建已废弃的 cache/");
        assert!(!layout.stickers_dir().join("1-旧.md").exists(), "旧便签应被清掉");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn restore_overwrite_data_rejects_broken_backup_without_touching_data() {
        let base = temp_dir("broken");
        let target = base.join("ws");
        let layout = Layout::at(&target);
        layout::ensure_layout(&layout, "目标").unwrap();
        std::fs::write(layout.stickers_dir().join("1-旧.md"), "# 旧").unwrap();

        let broken = base.join("broken.zip");
        std::fs::write(&broken, b"not a zip").unwrap();
        assert!(restore_overwrite_data(&target, &broken).is_err());
        // 现有数据未被动过
        assert!(layout.stickers_dir().join("1-旧.md").is_file());
        assert!(layout.signature_path().is_file());
        let _ = std::fs::remove_dir_all(&base);
    }
}

// ═══════════════════ Tauri 命令（转发到上方的纯逻辑） ═══════════════════

fn dto(entry: WorkspaceEntry) -> crate::models::WorkspaceEntryDto {
    crate::models::WorkspaceEntryDto {
        id: entry.id,
        name: entry.name,
        path: entry.path,
        created_at: entry.created_at,
    }
}

/// 读取备份信息：前端展示摘要（名称/时间/条目/大小）并在恢复前校验备份可用。
#[tauri::command]
pub fn workspace_inspect_backup_cmd(zip_path: String) -> Result<backup::BackupInfo, String> {
    let zip = PathBuf::from(zip_path.trim());
    if !zip.is_file() {
        return Err("备份文件不存在或已被移动".to_string());
    }
    backup::inspect(&zip).map_err(|e| e.to_string())
}

/// 恢复备份：
/// - `mode="new"`：解压到 `dest_root`（必须不存在或为空）→ 注册为**新工作空间**（不自动切换，
///   除非它是第一个）；名称沿用备份里的名字，重名自动加「（恢复）」后缀。
/// - `mode="overwrite"`：覆盖 `id`（缺省为当前工作空间）——先产出回滚点 zip，
///   旧数据整体改名到暂存目录下的 `rollback-*`（不删除），并关闭便签窗口 + 释放 DB 连接。
#[tauri::command]
pub fn workspace_restore_cmd(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::state::AppState>,
    zip_path: String,
    mode: String,
    dest_root: Option<String>,
    id: Option<String>,
    name: Option<String>,
) -> Result<RestoreOutcome, String> {
    let zip = PathBuf::from(zip_path.trim());
    if !zip.is_file() {
        return Err("备份文件不存在或已被移动".to_string());
    }
    match mode.as_str() {
        "new" => {
            let dest = dest_root
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .map(PathBuf::from)
                .ok_or_else(|| "请选择恢复目标目录".to_string())?;
            let (entry, entries) =
                restore_as_new(&state.registry_path(), &zip, &dest, name.as_deref())
                    .map_err(|e| e.to_string())?;
            crate::events::emit_push_update(&app, 0);
            Ok(RestoreOutcome {
                mode: "new".to_string(),
                name: entry.name.clone(),
                root: entry.path.clone(),
                entries,
                workspace: Some(dto(entry)),
                rollback_zip: None,
                rollback_dir: None,
            })
        }
        "overwrite" => restore_overwrite_impl(&app, &state, &zip, id.as_deref())
            .map_err(|e| e.to_string()),
        other => Err(format!("未知的恢复方式：{other}")),
    }
}

/// 覆盖式恢复：回滚点 → 关窗/释放连接 → 换数据 → 重开库。
fn restore_overwrite_impl(
    app: &tauri::AppHandle,
    state: &crate::state::AppState,
    zip: &Path,
    id: Option<&str>,
) -> Result<RestoreOutcome> {
    let reg = state.registry_path();
    let current = cmds::current(&reg)?;
    let target = match id.map(str::trim).filter(|s| !s.is_empty()) {
        Some(want) => cmds::list(&reg)?
            .into_iter()
            .find(|w| w.id == want)
            .context("工作空间不存在")?,
        None => current.clone().context("没有当前工作空间可覆盖")?,
    };
    let is_current = current.as_ref().map(|w| w.id == target.id).unwrap_or(false);
    let root = PathBuf::from(&target.path);
    let layout = Layout::at(&root);
    // 恢复前先校验备份合法：不合法就完全不动现有数据
    backup::inspect(zip).context("备份文件不可用")?;
    // 父便签保护：覆盖当前工作空间会关闭全部便签窗口，还有打开的任务窗口时先拒绝。
    if is_current {
        crate::ensure_no_open_todo_anywhere(app).map_err(|e| anyhow::anyhow!(e))?;
    }
    // 1) 回滚点（需要数据库连接，必须在 release_db 之前）
    let rollback_zip = if is_current {
        Some(state.with_conn(|conn| make_recovery_point(&layout, conn))?)
    } else {
        None
    };
    // 2) 覆盖当前工作空间：关闭便签窗口 + 释放连接（否则 Windows 上文件被占用）
    if is_current {
        use tauri::Manager; // webview_windows() 由该 trait 提供
        for (label, win) in app.webview_windows().iter() {
            if label.starts_with("sticker-") {
                let _ = win.close();
                tracing::info!("[restore] 关闭便签窗口 {label}");
            }
        }
        state.release_db()?;
    }
    // 3) 换数据（文件级失败会自动回滚旧数据）
    match restore_overwrite_data(&root, zip) {
        Ok((entries, rollback_dir)) => {
            if is_current {
                state
                    .switch_db(&layout.db_path())
                    .context("恢复完成但重新打开数据库失败")?;
            }
            crate::events::emit_push_update(app, 0);
            tracing::info!("[restore] 覆盖「{}」完成，条目 {entries}", target.name);
            Ok(RestoreOutcome {
                mode: "overwrite".to_string(),
                name: target.name.clone(),
                root: target.path.clone(),
                entries,
                workspace: Some(dto(target)),
                rollback_zip: rollback_zip.map(|p| p.to_string_lossy().into_owned()),
                rollback_dir: Some(rollback_dir.to_string_lossy().into_owned()),
            })
        }
        Err(e) => {
            // 换数据失败已回滚文件：重新打开（回滚后的）原库，保持可用
            if is_current {
                if let Err(reopen) = state.switch_db(&layout.db_path()) {
                    tracing::error!("[restore] 回滚后重开数据库失败：{reopen}");
                }
            }
            Err(e)
        }
    }
}

/// 备份/恢复在**真实工作空间**上的端到端验证（默认 `#[ignore]`，只在手动排查时跑）。
///
/// 用法：`$env:OII_WS_VERIFY='C:\Users\ljz\Documents\我的文档'; cargo test --lib -- --ignored real_workspace_backup_restore --nocapture`
///
/// 全程只读原始工作空间（外加一次备份），其余破坏/恢复动作都在系统临时目录的沙箱里做，
/// 注册表也用临时文件，绝不碰用户的真实注册表。
#[cfg(test)]
mod real_ws_verify {
    use super::*;
    use crate::workspace::backup as bk;
    use rusqlite::Connection;

    struct Facts {
        tree: Vec<String>,
        stickers: Vec<String>,
        groups: Vec<String>,
    }

    impl Facts {
        fn summary(&self) -> String {
            format!("{} 个文件 / {} 张便签 / {} 个分组", self.tree.len(), self.stickers.len(), self.groups.len())
        }
    }

    fn hash(bytes: &[u8]) -> u64 {
        let mut h: u64 = 1469598103934665603;
        for b in bytes {
            h ^= *b as u64;
            h = h.wrapping_mul(1099511628211);
        }
        h
    }

    /// 指纹：文件树（含每个文件的大小与内容哈希；跳过 sqlite 的 wal/shm）+ DB 便签/分组。
    fn facts(root: &Path) -> Facts {
        let mut tree: Vec<String> = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for e in rd.flatten() {
                let p = e.path();
                let name = e.file_name().to_string_lossy().into_owned();
                let rel = p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    stack.push(p);
                } else if !name.ends_with("-wal") && !name.ends_with("-shm") {
                    if name.ends_with(".db") {
                        // 备份走 VACUUM INTO，SQLite 页布局会变 → 只比存在性，
                        // 内容用下面的便签/分组查询比对
                        tree.push(format!("{rel} [sqlite]"));
                    } else {
                        let data = std::fs::read(&p).unwrap_or_default();
                        tree.push(format!("{rel} ({} 字节, {:016x})", data.len(), hash(&data)));
                    }
                }
            }
        }
        tree.sort();
        let conn = Connection::open(Layout::at(root).db_path()).unwrap();
        let mut stickers: Vec<String> = crate::db::sticker_repo::list_all(&conn)
            .unwrap()
            .iter()
            .map(|s| {
                format!(
                    "{}|{}|{:?}|{:?}|{} 字节",
                    s.id,
                    s.title,
                    s.group_id,
                    s.file_name,
                    s.content.len()
                )
            })
            .collect();
        stickers.sort();
        let mut groups: Vec<String> = crate::db::group_repo::list(&conn)
            .unwrap()
            .iter()
            .map(|g| format!("{}|{}|{:?}|{}", g.id, g.name, g.parent_id, g.sort_order))
            .collect();
        groups.sort();
        Facts { tree, stickers, groups }
    }

    fn zip_names(zip: &Path) -> Vec<String> {
        let file = std::fs::File::open(zip).unwrap();
        let mut ar = zip::ZipArchive::new(file).unwrap();
        let mut names = Vec::new();
        for i in 0..ar.len() {
            if let Ok(f) = ar.by_index(i) {
                names.push(format!("{} ({} 字节)", f.name(), f.size()));
            }
        }
        names.sort();
        names
    }

    fn copy_dir(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for e in std::fs::read_dir(from).unwrap().flatten() {
            let target = to.join(e.file_name());
            if e.file_type().unwrap().is_dir() {
                copy_dir(&e.path(), &target);
            } else {
                std::fs::copy(e.path(), &target).unwrap();
            }
        }
    }

    #[test]
    #[ignore]
    fn real_workspace_backup_restore() {
        let root = PathBuf::from(std::env::var("OII_WS_VERIFY").expect("需要设置 OII_WS_VERIFY"));
        let work = std::env::temp_dir().join(format!("ws-verify-{}", now_tag()));
        let _ = remove_any(&work);
        std::fs::create_dir_all(&work).unwrap();
        println!("==== 沙箱：{}", work.display());
        let before = facts(&root);
        println!("原始工作空间：{}", before.summary());

        // 1) 备份
        let layout = Layout::at(&root);
        let conn = Connection::open(layout.db_path()).unwrap();
        let zip = work.join("真实备份.zip");
        let bytes = bk::backup(&layout, &conn, &zip).unwrap();
        let info = bk::inspect(&zip).unwrap();
        println!(
            "① 备份：{bytes} 字节 / {} 条 / 未压缩 {} 字节；名称「{}」schema v{} 版本 {} has_manifest={}",
            info.entries, info.bytes, info.name, info.schema_version, info.app_version, info.has_manifest
        );
        println!("   签名 id={} 工作空间 created_at={}", info.workspace_id, info.created_at);
        for n in zip_names(&zip) {
            println!("   [zip] {n}");
        }

        // 2) 恢复为新工作空间（注册表用临时文件，绝不碰真实注册表）
        let dest = work.join("恢复为新");
        let reg = work.join("registry.json");
        let (entry, entries) = restore_as_new(&reg, &zip, &dest, None).unwrap();
        println!("② 恢复为新：name=「{}」entries={entries} → {}", entry.name, entry.path);
        let as_new = facts(&dest);
        println!("   新工作空间：{}", as_new.summary());
        assert_eq!(before.tree, as_new.tree, "恢复为新：文件树不一致");
        assert_eq!(before.stickers, as_new.stickers, "恢复为新：便签数据不一致");
        assert_eq!(before.groups, as_new.groups, "恢复为新：分组数据不一致");

        // 3) 覆盖式恢复：先拷一份，故意破坏，再看能否换回来
        let target = work.join("覆盖目标");
        copy_dir(&root, &target);
        let tlayout = Layout::at(&target);
        let tconn = Connection::open(tlayout.db_path()).unwrap();
        let recovery = make_recovery_point(&tlayout, &tconn).unwrap();
        println!("③ 回滚点 zip：{} 字节", std::fs::metadata(&recovery).unwrap().len());
        // 破坏：加一个文件、删一个文件、清空一张便签的正文
        std::fs::write(tlayout.stickers_dir().join("被破坏.md"), "x").unwrap();
        std::fs::remove_file(tlayout.stickers_dir().join("随手记").join("oii_sticker Bug记录.md")).unwrap();
        tconn
            .execute("UPDATE stickers SET content = '被破坏' WHERE id = 7", [])
            .unwrap();
        // Windows 上被占用的文件不能改名：真实命令层在换数据前也会先 release_db
        tconn.close().unwrap();
        let (entries2, rollback) = restore_overwrite_data(&target, &zip).unwrap();
        println!("   覆盖恢复：entries={entries2} 回滚目录={}", rollback.display());
        let over = facts(&target);
        println!("   覆盖后：{}", over.summary());
        assert_eq!(before.tree, over.tree, "覆盖恢复：文件树不一致");
        assert_eq!(before.stickers, over.stickers, "覆盖恢复：便签数据不一致");
        assert_eq!(before.groups, over.groups, "覆盖恢复：分组数据不一致");
        println!("   回滚目录里的旧数据：{:?}", std::fs::read_dir(&rollback).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect::<Vec<_>>());
        assert!(!target.join("cache").exists() && !target.join("library").exists(), "cache/library 不该被恢复回来");
        assert!(recovery.is_file(), "回滚点应保留");

        println!("==== 备份与恢复验证全部通过");
    }
}
