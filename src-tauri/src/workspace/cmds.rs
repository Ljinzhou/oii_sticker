//! 工作空间命令核心逻辑（注册表读写 + 目录操作）。
//! Tauri 包装（需 app/state）在 lib.rs 中。

use anyhow::{bail, Context, Result};
use std::path::Path;

use super::layout::*;

pub fn current(state_registry_path: &Path) -> Result<Option<WorkspaceEntry>> {
    let reg = load_registry(state_registry_path)?;
    let Some(id) = reg.current else { return Ok(None) };
    Ok(reg.workspaces.into_iter().find(|w| w.id == id))
}

/// 新建工作空间：建目录结构 + 写入注册表；无当前项时自动激活。
pub fn create(reg_path: &Path, root: &Path, name: Option<&str>) -> Result<WorkspaceEntry> {
    let mut reg = load_registry(reg_path)?;
    let id = format!("w-{}", gen_id_hex());
    let name = name.filter(|n| !n.trim().is_empty()).unwrap_or("未命名工作空间").to_string();
    let layout = Layout::at(root);
    ensure_layout(&layout, &name).context("创建工作空间目录失败")?;
    let entry = WorkspaceEntry {
        id: id.clone(),
        name,
        path: root.to_string_lossy().into_owned(),
        created_at: now_iso(),
    };
    reg.workspaces.push(entry.clone());
    if reg.current.is_none() {
        reg.current = Some(id);
    }
    save_registry(reg_path, &reg)?;
    Ok(entry)
}

/// 切换当前工作空间（不负责 DB 重连，由调用方 switch_db）。
pub fn switch(reg_path: &Path, id: &str) -> Result<WorkspaceEntry> {
    let mut reg = load_registry(reg_path)?;
    let entry = reg
        .workspaces
        .iter()
        .find(|w| w.id == id)
        .cloned()
        .context("工作空间不存在")?;
    reg.current = Some(id.to_string());
    save_registry(reg_path, &reg)?;
    Ok(entry)
}

/// 转移用：更新注册表 entry.path（+ save）；返回更新后条目。不改 current。
pub fn relocate(reg_path: &Path, id: &str, new_path: &Path) -> Result<WorkspaceEntry> {
    let mut reg = load_registry(reg_path)?;
    let entry = reg
        .workspaces
        .iter_mut()
        .find(|w| w.id == id)
        .context("工作空间不存在")?;
    entry.path = new_path.to_string_lossy().into_owned();
    let entry = entry.clone();
    save_registry(reg_path, &reg)?;
    Ok(entry)
}

/// 销毁：删注册表项 + 目录；最后一个或当前项拒绝。
pub fn destroy(reg_path: &Path, id: &str) -> Result<()> {
    let mut reg = load_registry(reg_path)?;
    if reg.workspaces.len() <= 1 {
        bail!("至少需要保留一个工作空间，无法删除最后一个");
    }
    if reg.current.as_deref() == Some(id) {
        bail!("不能删除当前激活的工作空间，请先切换到其他工作空间");
    }
    let entry = reg.workspaces.iter().find(|w| w.id == id).cloned().context("工作空间不存在")?;
    reg.workspaces.retain(|w| w.id != id);
    save_registry(reg_path, &reg)?;
    std::fs::remove_dir_all(&entry.path).with_context(|| format!("删除目录失败：{}", entry.path))?;
    Ok(())
}

pub fn list(reg_path: &Path) -> Result<Vec<WorkspaceEntry>> {
    Ok(load_registry(reg_path)?.workspaces)
}

fn gen_id_hex() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let millis = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
    format!("{millis:x}")
}

fn now_iso() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    format!("{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_env(tag: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("ws-cmds-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        (dir.clone(), dir.join("app").join("workspaces.json"))
    }

    #[test]
    fn create_activates_first_and_list_shows_entry() {
        let (dir, reg) = temp_env("create");
        let root = dir.join("ws");
        let e = create(&reg, &root, Some("工作区一")).unwrap();
        assert_eq!(current(&reg).unwrap().unwrap().id, e.id);
        assert!(list(&reg).unwrap().len() == 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn relocate_updates_entry_path_and_persists() {
        let (dir, reg) = temp_env("relocate");
        let e = create(&reg, &dir.join("ws-old"), Some("A")).unwrap();
        let new_path = dir.join("ws-new");
        let updated = relocate(&reg, &e.id, &new_path).unwrap();
        assert_eq!(updated.path, new_path.to_string_lossy());
        assert_eq!(current(&reg).unwrap().unwrap().id, e.id, "relocate 不改 current");
        let reloaded = list(&reg).unwrap().pop().unwrap();
        assert_eq!(reloaded.path, new_path.to_string_lossy(), "路径已持久化");
        assert!(relocate(&reg, "w-nonexistent", &new_path).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn switch_to_second_and_destroy_first_keeps_last_protected() {
        let (dir, reg) = temp_env("switch");
        let a = create(&reg, &dir.join("ws-a"), Some("A")).unwrap();
        let b = create(&reg, &dir.join("ws-b"), Some("B")).unwrap();
        assert_eq!(current(&reg).unwrap().unwrap().id, a.id, "首个自动激活");
        switch(&reg, &b.id).unwrap();
        assert_eq!(current(&reg).unwrap().unwrap().id, b.id);
        // 当前项不能被销毁
        assert!(destroy(&reg, &b.id).is_err());
        destroy(&reg, &a.id).unwrap();
        assert!(!dir.join("ws-a").exists());
        // 最后一个不能销毁
        assert!(destroy(&reg, &b.id).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
