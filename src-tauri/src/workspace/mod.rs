use std::path::{Path, PathBuf};

use anyhow::Result;
use layout::{load_registry, save_registry, Layout, Registry};

pub mod backup;
pub mod cmds;
pub mod layout;
pub mod md_store;

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
    // 无论哪种情况都把失效项从清单里移除。
    let before = reg.workspaces.len();
    reg.workspaces.retain(|w| is_valid_workspace(Path::new(&w.path)));
    let removed_count = before - reg.workspaces.len();
    if removed_count > 0 {
        save_registry(&reg_path, &reg)?;
    }

    Ok(BootResolution {
        db_path: Some(Layout::at(Path::new(&valid[0].path)).db_path()),
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
