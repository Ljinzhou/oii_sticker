use std::path::{Path, PathBuf};

use layout::{load_registry};

pub mod cmds;
pub mod layout;

/// 启动用 DB 路径：注册表有当前工作控件 → 该工作控件库；否则 bootstrap 临时库。
/// 只读注册表，不写；首次引导完成后才写。
pub fn boot_db_path(app_data_dir: &Path) -> PathBuf {
    let reg_path = app_data_dir.join("workspaces.json");
    if let Ok(reg) = load_registry(&reg_path) {
        if let (Some(id), Some(entry)) = (reg.current.as_deref().map(|s| s.to_string()), reg.workspaces.iter().find(|w| Some(w.id.as_str()) == reg.current.as_deref())) {
            let _ = id;
            return PathBuf::from(&entry.path).join("data").join("index.db");
        }
    }
    app_data_dir.join("bootstrap").join("bootstrap.db")
}

/// 旧版单库路径（迁移用，位于 app_data_dir/stickers.db）。
pub fn legacy_db_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("stickers.db")
}
