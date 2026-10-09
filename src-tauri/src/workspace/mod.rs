use std::path::{Path, PathBuf};

use anyhow::Result;
use layout::{load_registry, save_registry, Layout, Registry};

pub mod backup;
pub mod cmds;
pub mod layout;
pub mod md_store;
pub mod naming;
pub mod restore;
pub mod sync;

/// 工作控件目录有效性：签名文件与数据库至少其一存在。
/// （两者都缺失 = 目录被删除/清空，注册表记录已失效。）
pub fn is_valid_workspace(root: &Path) -> bool {
    let layout = Layout::at(root);
    layout.signature_path().is_file() || layout.db_path().is_file()
}

/// 启动引导结果：
/// - `Some(db)`：有效工作控件，直接打开该库；
/// - `None`：没有任何有效工作控件（前端进入首次引导；启动路径不落盘建库）。
pub struct BootResolution {
    pub db_path: Option<PathBuf>,
    /// 因目录失效而从注册表移除的工作控件（供日志）。
    pub removed_invalid: Vec<String>,
}

/// 启动解析：
/// 1. 校验注册表中的工作控件目录是否真实存在，失效项移除；
/// 2. 当前项有效 → 用它；否则回退到第一个有效项并设为当前；
/// 3. 一个有效项都没有 → None（前端引导创建），绝不无声重建目录。
pub fn resolve_boot_workspace(app_data_dir: &Path) -> Result<BootResolution> {
    let reg_path = app_data_dir.join("workspaces.json");
    let mut reg: Registry = load_registry(&reg_path)?;
    let mut removed_invalid = Vec::new();
    // 克隆有效项，避免借用冲突（后续要对 reg.workspaces 做 retain 清理）。
    let valid: Vec<layout::WorkspaceEntry> = reg
        .workspaces
        .iter()
        .filter(|w| {
            let ok = is_valid_workspace(Path::new(&w.path));
            if !ok {
                removed_invalid.push(format!("{}({})", w.name, w.path));
            }
            ok
        })
        .cloned()
        .collect();

    if valid.is_empty() {
        // 全部失效/无记录：重置注册表为空，交由前端首次引导。
        reg.workspaces.clear();
        reg.current = None;
        save_registry(&reg_path, &reg)?;
        return Ok(BootResolution { db_path: None, removed_invalid });
    }

    // 当前项失效 → 回退到第一个有效项并更新 current。
    let current_valid = reg
        .current
        .as_deref()
        .and_then(|id| valid.iter().find(|w| w.id == id))
        .is_some();
    if !current_valid {
        reg.current = Some(valid[0].id.clone());
    }
    // 实际要打开的那一项：current 有效就用 current，否则回退 valid[0]。
    // 下面返回 db_path 与修正后的 current 必须来自同一条目，否则
    // commands::current()（读注册表）与实际打开的库会指向不同工作空间。
    let chosen = match reg.current.as_deref().and_then(|id| valid.iter().find(|w| w.id == id)) {
        Some(entry) => entry,
        None => &valid[0],
    };
    // 无论哪种情况都把失效项从清单里移除。
    let before = reg.workspaces.len();
    reg.workspaces.retain(|w| is_valid_workspace(Path::new(&w.path)));
    let removed_count = before - reg.workspaces.len();
    // 任一改动都要落盘：只按"移除了失效项"来保存会漏掉 current 的修正
    // （current 为 None 或指向已失效项且清单本就没有失效项时），
    // 那样本次启动用了回退项，下次启动又会重新算一遍。
    if removed_count > 0 || !current_valid {
        save_registry(&reg_path, &reg)?;
    }

    Ok(BootResolution {
        db_path: Some(Layout::at(Path::new(&chosen.path)).db_path()),
        removed_invalid,
    })
}

/// 清理程序级目录里的历史遗留数据文件（旧版单库、bootstrap 临时库）。
/// 数据全部以工作控件为单位存放后，这些文件不应再出现。
pub fn cleanup_legacy_artifacts(app_data_dir: &Path) {
    let legacy = app_data_dir.join("stickers.db");
    for path in [
        legacy.clone(),
        PathBuf::from(format!("{}-wal", legacy.display())),
        PathBuf::from(format!("{}-shm", legacy.display())),
    ] {
        if path.exists() {
            let _ = std::fs::remove_file(&path);
        }
    }
    let bootstrap = app_data_dir.join("bootstrap");
    if bootstrap.is_dir() {
        let _ = std::fs::remove_dir_all(&bootstrap);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 在 `workspaces.json` 里枚举相对目录名即可造出有效/失效工作控件：
    /// 名字带 `missing` 的一律不创建目录（目录不存在 = 失效）。
    fn resolve_registry(tag: &str, names: &[&str], current: Option<&str>) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ws-boot-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut workspaces = Vec::new();
        for name in names {
            let id = format!("w-{name}");
            let path = if name.contains("missing") {
                dir.join(name)
            } else {
                let root = dir.join(name);
                layout::ensure_layout(&layout::Layout::at(&root), name).unwrap();
                std::fs::create_dir_all(root.join("data")).unwrap();
                std::fs::write(root.join("data").join("index.db"), b"").unwrap();
                root
            };
            workspaces.push(layout::WorkspaceEntry {
                id,
                name: (*name).to_string(),
                path: path.to_string_lossy().into_owned(),
                created_at: "1".into(),
            });
        }
        let reg = layout::Registry {
            current: current.map(|c| format!("w-{c}")),
            workspaces,
        };
        layout::save_registry(&dir.join("workspaces.json"), &reg).unwrap();
        dir
    }

    /// current 指向的工作控件有效、但不是清单第一项 → 必须选中 current，
    /// 且不能把 current 改写成第一项。
    #[test]
    fn resolve_boot_workspace_prefers_valid_current_over_first_entry() {
        let dir = resolve_registry("current-not-first", &["ws-a", "ws-b", "ws-c"], Some("ws-c"));
        let boot = resolve_boot_workspace(&dir).unwrap();
        assert_eq!(
            boot.db_path.as_deref(),
            Some(dir.join("ws-c").join("data").join("index.db")).as_deref(),
            "current 有效就必须用它，而不是清单第一项"
        );
        assert_eq!(boot.removed_invalid.len(), 0);
        let after = layout::load_registry(&dir.join("workspaces.json")).unwrap();
        assert_eq!(after.current.as_deref(), Some("w-ws-c"), "current 不应被改写");
        assert_eq!(after.workspaces.len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// current 缺失（None）或指向失效项 → 回退第一个有效项，并写回注册表。
    #[test]
    fn resolve_boot_workspace_falls_back_to_first_valid_when_current_unusable() {
        // current 缺失
        let dir = resolve_registry("current-none", &["ws-a", "ws-b"], None);
        let boot = resolve_boot_workspace(&dir).unwrap();
        assert_eq!(
            boot.db_path.as_deref(),
            Some(dir.join("ws-a").join("data").join("index.db")).as_deref(),
            "无 current 时回退第一项"
        );
        let after = layout::load_registry(&dir.join("workspaces.json")).unwrap();
        assert_eq!(after.current.as_deref(), Some("w-ws-a"), "回退后写回 current");
        let _ = std::fs::remove_dir_all(&dir);

        // current 指向失效项（目录不存在）→ 失效项被移除 + 回退到第一项
        let dir = resolve_registry("current-missing", &["ws-a", "ws-missing"], Some("ws-missing"));
        let boot = resolve_boot_workspace(&dir).unwrap();
        assert_eq!(
            boot.db_path.as_deref(),
            Some(dir.join("ws-a").join("data").join("index.db")).as_deref(),
            "current 失效时回退第一项"
        );
        assert_eq!(boot.removed_invalid.len(), 1);
        let after = layout::load_registry(&dir.join("workspaces.json")).unwrap();
        assert_eq!(after.current.as_deref(), Some("w-ws-a"));
        assert_eq!(after.workspaces.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 失效项排在 current 之前时仍选 current：这正是"恒取第一项"会踩的场景。
    #[test]
    fn resolve_boot_workspace_chooses_valid_entry_after_removed_invalid_one() {
        let dir = resolve_registry("invalid-first", &["ws-missing", "ws-a", "ws-b"], Some("ws-b"));
        let boot = resolve_boot_workspace(&dir).unwrap();
        assert_eq!(boot.removed_invalid.len(), 1);
        assert_eq!(
            boot.db_path.as_deref(),
            Some(dir.join("ws-b").join("data").join("index.db")).as_deref()
        );
        let after = layout::load_registry(&dir.join("workspaces.json")).unwrap();
        assert_eq!(after.current.as_deref(), Some("w-ws-b"));
        assert_eq!(after.workspaces.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
