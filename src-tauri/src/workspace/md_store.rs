//! 便签 md 文件读写（主存储）。路径均由 DB 的 file_name 派生，防穿越。

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use super::layout::{atomic_write, Layout, sticker_file_name};

/// 读取 md 内容；不存在返回 Ok(None)。
pub fn load(root: &Path, file_name: &str) -> Result<Option<String>> {
    let path = safe_join(&Layout::at(root).stickers_dir(), file_name)?;
    if !path.exists() { return Ok(None); }
    Ok(Some(std::fs::read_to_string(&path).with_context(|| format!("读取便签 md 失败：{}", path.display()))?))
}

/// 原子写入。
pub fn write(root: &Path, file_name: &str, content: &str) -> Result<()> {
    let path = safe_join(&Layout::at(root).stickers_dir(), file_name)?;
    atomic_write(&path, content.as_bytes())
}

pub fn remove(root: &Path, file_name: &str) -> Result<()> {
    let path = safe_join(&Layout::at(root).stickers_dir(), file_name)?;
    if path.exists() { std::fs::remove_file(&path)?; }
    Ok(())
}

/// 标题变化时重命名（旧名不存在则跳过），返回新文件名。
pub fn rename_for_title(root: &Path, old_name: &str, id: i64, title: &str) -> Result<String> {
    let new_name = sticker_file_name(id, title);
    if old_name == new_name { return Ok(new_name); }
    let old_path = safe_join(&Layout::at(root).stickers_dir(), old_name)?;
    if old_path.exists() {
        let new_path = safe_join(&Layout::at(root).stickers_dir(), &new_name)?;
        if new_path.exists() { std::fs::remove_file(&new_path)?; }
        std::fs::rename(&old_path, &new_path).with_context(|| format!("重命名便签 md 失败：{}", old_path.display()))?;
    }
    Ok(new_name)
}

/// 防目录穿越：仅允许普通文件名。
pub fn safe_join(dir: &Path, file_name: &str) -> Result<PathBuf> {
    if file_name.ends_with('/') || file_name.ends_with('\\') {
        anyhow::bail!("非法文件名：{file_name}");
    }
    let mut comps = Path::new(file_name).components();
    match comps.next() {
        Some(std::path::Component::Normal(_)) if comps.next().is_none() => {}
        _ => anyhow::bail!("非法文件名：{file_name}"),
    }
    Ok(dir.join(file_name))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("md-store-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let _ = super::super::layout::ensure_layout(&super::super::layout::Layout::at(&d), "t");
        d
    }

    #[test]
    fn write_load_remove_roundtrip() {
        let d = tmp("round");
        write(&d, "1-测试内容.md", "# 标题\n第二行").unwrap();
        assert_eq!(load(&d, "1-测试内容.md").unwrap().unwrap(), "# 标题\n第二行");
        assert!(load(&d, "不存在.md").unwrap().is_none());
        remove(&d, "1-测试内容.md").unwrap();
        assert!(load(&d, "1-测试内容.md").unwrap().is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn rename_for_title_renames_and_keeps_content() {
        let d = tmp("rename");
        write(&d, "1-旧名.md", "正文").unwrap();
        let new = rename_for_title(&d, "1-旧名.md", 1, "新名").unwrap();
        assert_eq!(new, "1-新名.md");
        assert_eq!(load(&d, "1-新名.md").unwrap().unwrap(), "正文");
        assert!(!d.join("stickers").join("1-旧名.md").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn safe_join_rejects_escape() {
        // 正常：单一文件名
        assert!(safe_join(Path::new("."), "a.md").unwrap().ends_with("a.md"));
        // 非法：穿越/路径
        assert!(safe_join(Path::new("."), "../x.md").is_err());
        assert!(safe_join(Path::new("."), "sub/y.md").is_err());
        assert!(safe_join(Path::new("."), "").is_err());
        assert!(safe_join(Path::new("."), "..").is_err());
        assert!(safe_join(Path::new("."), "a.md/").is_err());
    }
}
