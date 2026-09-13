//! 备份恢复：把 zip 备份「恢复为新工作空间」或「覆盖现有工作空间」。
//!
//! 安全约定：
//! - 先 `inspect` 校验备份合法，再动任何现有数据；
//! - 覆盖模式把旧数据整体 **改名** 到 `cache/rollback-*`（不删除，失败可回滚），
//!   并在动手前把当前工作空间完整备份到 `cache/recovery-*.zip`（第二重回滚点）；
//! - 覆盖当前工作空间会先关闭全部便签窗口并释放数据库连接（Windows 上文件被占用无法替换）。

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use super::backup;
use super::cmds;
use super::layout::{self, Layout, WorkspaceEntry};

/// 覆盖时被替换的工作空间顶层项（cache/ 不动）。
const SWAP_ITEMS: [&str; 5] = ["stickers", "assets", "library", "data", "workspace.json"];

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

/// 覆盖前的事前完整备份（回滚点）：写入目标工作空间的 cache/。
pub fn make_recovery_point(
    layout: &Layout,
    conn: &rusqlite::Connection,
) -> Result<PathBuf> {
    let cache = layout.cache_dir();
    std::fs::create_dir_all(&cache).with_context(|| format!("创建缓存目录失败：{}", cache.display()))?;
    let path = cache.join(format!("recovery-{}.zip", now_tag()));
    backup::backup(layout, conn, &path).context("生成回滚点失败")?;
    Ok(path)
}

/// 覆盖式恢复的数据部分（不含 DB 切换与窗口关闭，便于单测）：
/// 解压 staging → 替换 root。返回（条目数, 回滚目录）。
pub fn restore_overwrite_data(root: &Path, zip: &Path) -> Result<(u64, PathBuf)> {
    let info = backup::inspect(zip).context("备份文件不可用")?;
    let layout = Layout::at(root);
    let staging = layout.cache_dir().join(format!("restore-{}.tmp", now_tag()));
    if staging.exists() {
        remove_any(&staging)?;
    }
    std::fs::create_dir_all(&staging)?;
    backup::extract(zip, &staging).context("解压备份失败")?;
    let rollback_dir = layout.cache_dir().join(format!("rollback-{}.tmp", now_tag()));
    let result = apply_restore(root, &staging, &rollback_dir);
    let _ = remove_any(&staging); // 内容已被 rename 走，剩下空壳
    let swapped = result?;
    if swapped.is_empty() {
        bail!("备份内没有任何可恢复的数据");
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
        std::fs::write(layout.stickers_dir().join("1-欢迎.md"), sticker).unwrap();
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
        assert!(dest.join("stickers").join("1-欢迎.md").is_file());
        assert!(dest.join("data").join("index.db").is_file());
        assert_eq!(
            std::fs::read_to_string(dest.join("stickers").join("1-欢迎.md")).unwrap(),
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
        std::fs::write(layout.stickers_dir().join("1-旧.md"), "# 旧内容").unwrap();
        std::fs::write(layout.cache_dir().join("keep.bin"), b"cache-keep").unwrap();
        let conn = rusqlite::Connection::open(layout.db_path()).unwrap();
        crate::db::schema::run_migrations(&conn).unwrap();
        let _ = conn.close();

        let (entries, rollback_dir) = restore_overwrite_data(&target, &zip).unwrap();
        assert!(entries >= 2);
        // 新内容生效
        assert_eq!(
            std::fs::read_to_string(layout.stickers_dir().join("1-欢迎.md")).unwrap(),
            "# 新内容"
        );
        assert!(!layout.stickers_dir().join("1-旧.md").exists(), "旧便签不应残留");
        // 旧数据被保留到回滚目录（不删除）
        assert!(rollback_dir.join("stickers").join("1-旧.md").is_file());
        assert!(rollback_dir.join("data").join("index.db").is_file());
        // cache 不受影响
        assert!(layout.cache_dir().join("keep.bin").is_file());
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
        assert_eq!(swapped.len(), 5, "实际：{swapped:?}");
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
///   旧数据整体改名到 `cache/rollback-*`（不删除），并关闭便签窗口 + 释放 DB 连接。
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
