//! 工作空间内的文件路径与便签 md 读写（md 是主存储）。
//!
//! 路径一律是**相对根目录的相对路径**，用 `/` 分隔，例如：
//! - 便签正文：相对 `stickers/` 的 `随手记/oii_sticker Bug记录.md`（分组即子目录）
//! - 附件：相对 `assets/` 的 `随手记/oii_sticker Bug记录_14/粘贴-20261006-101500.png`
//!
//! [`rel_join`] 是唯一入口，逐段校验，杜绝 `..` 穿越与绝对路径注入。

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use super::layout::{atomic_write, Layout};

/// 逐段校验相对路径并拼到 `dir` 下。
///
/// 允许嵌套（分组子目录），但每一段都必须是普通文件名：
/// 空段、`.`、`..`、绝对路径、含系统禁用字符的段一律拒绝。
pub fn rel_join(dir: &Path, rel: &str) -> Result<PathBuf> {
    let rel = rel.trim().replace('\\', "/");
    if rel.is_empty() {
        bail!("非法路径：空路径");
    }
    if rel.starts_with('/') || rel.contains(':') {
        bail!("非法路径：{rel}");
    }
    let mut out = dir.to_path_buf();
    for part in rel.split('/') {
        if part.is_empty() {
            bail!("非法路径：{rel}");
        }
        if part == "." || part == ".." {
            bail!("非法路径：{rel}");
        }
        if part.chars().any(|c| c.is_control() || super::naming::is_forbidden_char(c)) {
            bail!("非法路径：{rel}");
        }
        out.push(part);
    }
    Ok(out)
}

/// 便签 md 的绝对路径（`<root>/stickers/<rel>`）。
pub fn sticker_path(root: &Path, rel: &str) -> Result<PathBuf> {
    rel_join(&Layout::at(root).stickers_dir(), rel)
}

/// 附件的绝对路径（`<root>/assets/<rel>`）。
pub fn asset_path(root: &Path, rel: &str) -> Result<PathBuf> {
    rel_join(&Layout::at(root).assets_dir(), rel)
}

/// 读取 md 内容；不存在返回 Ok(None)。
pub fn load(root: &Path, rel: &str) -> Result<Option<String>> {
    let path = sticker_path(root, rel)?;
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(
        std::fs::read_to_string(&path).with_context(|| format!("读取便签 md 失败：{}", path.display()))?,
    ))
}

/// 原子写入（父目录自动创建）。
pub fn write(root: &Path, rel: &str, content: &str) -> Result<()> {
    let path = sticker_path(root, rel)?;
    atomic_write(&path, content.as_bytes())
}

/// 删除 md（不存在则跳过；空出来的分组目录保留，它对应分组本身）。
pub fn remove(root: &Path, rel: &str) -> Result<()> {
    let path = sticker_path(root, rel)?;
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

/// 移动/重命名文件或目录（父目录自动创建）。
///
/// 同卷 `std::fs::rename`；跨卷时退化为「复制 + 删源」。
///
/// **绝不覆盖已有内容**：目标已存在（且不是同一个文件）时直接报错，交给调用方决定怎么办
/// （同步线程保留原位置、命令层提示用户改名）。唯一例外是目标是**空目录**——「改名到一个
/// 已经建好的空文件夹」是正常情形，删掉这个空壳不丢任何数据。
///
/// 这条"绝不覆盖"的规矩是必须的：本函数的所有调用方都在用户数据路径上。旧实现是
/// 「`to.exists()` 就 `remove_any(to)` 再 rename」，而 Windows 上 `笔记.MD` 与 `笔记.md`
/// 指向**同一个文件**（`to.exists()` 为真，但 `from == to` 的路径字符串比较为假）→
/// 会把源文件自己删掉，随后 rename 失败，最终该便签连附件一起被同步线程判为"用户在资源
/// 管理器里删了 md"而删除；目标被别的便签占用时还会静默吞掉别人的正文。
pub fn move_path(from: &Path, to: &Path) -> Result<()> {
    move_impl(from, to, false)
}

/// 移动/重命名**附件目录**专用：目标同名时直接替换。
///
/// 附件目录名里带着便签 id（`<标题>_<id>`），撞名只可能是同一张便签自己留下的旧目录，
/// 覆盖它是安全的（而且必须覆盖，否则改名会失败）。其余场景请用 [`move_path`]。
pub fn move_path_replacing(from: &Path, to: &Path) -> Result<()> {
    move_impl(from, to, true)
}

fn move_impl(from: &Path, to: &Path, replace: bool) -> Result<()> {
    if same_path(from, to) {
        return Ok(());
    }
    if !from.exists() {
        return Ok(());
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建目录失败：{}", parent.display()))?;
    }
    if to.exists() {
        if replace {
            remove_any(to)?;
        } else if to.is_dir() {
            // 只允许"空目录"：改名到已经建好的空文件夹是正常操作。
            let empty = std::fs::read_dir(to)
                .map(|mut it| it.next().is_none())
                .unwrap_or(false);
            if !empty {
                bail!("目标已存在且不是空目录，未改动原文件：{}", to.display());
            }
            std::fs::remove_dir(to)?;
        } else {
            bail!("目标已存在，未覆盖：{}", to.display());
        }
    }
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(_) => {
            // 跨卷（EXDEV）：目录递归复制后删源，文件直接 copy
            if from.is_dir() {
                copy_tree(from, to)?;
                std::fs::remove_dir_all(from)?;
            } else {
                std::fs::copy(from, to)?;
                std::fs::remove_file(from)?;
            }
            Ok(())
        }
    }
}

/// 两个路径是否指向同一个文件/目录。
///
/// 先比字符串，再用 `canonicalize()` 比实体路径（足够覆盖 Windows 的大小写不敏感与 8.3 短名）；
/// `canonicalize` 失败（路径还不存在等）时退化为不区分大小写的字符串比较。
pub fn same_path(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    if let (Ok(ca), Ok(cb)) = (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        if ca == cb {
            return true;
        }
    }
    a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy())
}

/// 删除文件或目录（存在才删）。
pub fn remove_any(path: &Path) -> Result<()> {
    if path.is_dir() {
        std::fs::remove_dir_all(path)?;
    } else if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("md-store-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let _ = super::super::layout::ensure_layout(&super::super::layout::Layout::at(&d), "t");
        d
    }

    #[test]
    fn write_load_remove_roundtrip_in_group_folder() {
        let d = tmp("round");
        write(&d, "随手记/测试内容.md", "# 标题\n第二行").unwrap();
        assert!(d.join("stickers").join("随手记").join("测试内容.md").is_file());
        assert_eq!(load(&d, "随手记/测试内容.md").unwrap().unwrap(), "# 标题\n第二行");
        assert!(load(&d, "随手记/不存在.md").unwrap().is_none());
        remove(&d, "随手记/测试内容.md").unwrap();
        assert!(load(&d, "随手记/测试内容.md").unwrap().is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn write_accepts_ungrouped_root_path() {
        let d = tmp("root");
        write(&d, "顶层便签.md", "正文").unwrap();
        assert!(d.join("stickers").join("顶层便签.md").is_file());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn write_accepts_backslash_separator() {
        let d = tmp("backslash");
        write(&d, "学习\\英语\\单词.md", "正文").unwrap();
        assert!(d.join("stickers").join("学习").join("英语").join("单词.md").is_file());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn rel_join_rejects_escape_and_absolute() {
        assert!(rel_join(Path::new("."), "a/b.md").is_ok());
        assert!(rel_join(Path::new("."), "../x.md").is_err());
        assert!(rel_join(Path::new("."), "a/../../x.md").is_err());
        assert!(rel_join(Path::new("."), "./a.md").is_err());
        assert!(rel_join(Path::new("."), "").is_err());
        assert!(rel_join(Path::new("."), "a.md/").is_err());
        assert!(rel_join(Path::new("."), "C:/x.md").is_err());
        assert!(rel_join(Path::new("."), "/x.md").is_err());
        assert!(rel_join(Path::new("."), "a?b.md").is_err());
    }

    #[test]
    fn move_path_renames_across_folders_and_creates_parents() {
        let d = tmp("move");
        write(&d, "随手记/旧名.md", "正文").unwrap();
        let from = sticker_path(&d, "随手记/旧名.md").unwrap();
        let to = sticker_path(&d, "学习/新名.md").unwrap();
        move_path(&from, &to).unwrap();
        assert!(!from.exists());
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "正文");
        // 源不存在 → 静默成功（幂等）
        move_path(&from, &to).unwrap();
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn move_path_handles_directories() {
        let d = tmp("movedir");
        let from = d.join("assets").join("旧组").join("便签_1");
        std::fs::create_dir_all(&from).unwrap();
        std::fs::write(from.join("a.png"), b"png").unwrap();
        let to = d.join("assets").join("新组").join("便签_1");
        move_path(&from, &to).unwrap();
        assert!(!from.exists());
        assert_eq!(std::fs::read_to_string(to.join("a.png")).unwrap(), "png");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 目标已有别的便签的 md：绝不覆盖（旧实现会先删掉目标，静默吞掉对方正文）。
    #[test]
    fn move_path_refuses_to_overwrite_existing_file() {
        let d = tmp("refuse");
        write(&d, "甲.md", "甲的正文").unwrap();
        write(&d, "乙.md", "乙的正文").unwrap();
        let from = sticker_path(&d, "甲.md").unwrap();
        let to = sticker_path(&d, "乙.md").unwrap();
        let err = move_path(&from, &to).unwrap_err().to_string();
        assert!(err.contains("未覆盖"), "错误信息应说明未覆盖：{err}");
        assert_eq!(load(&d, "甲.md").unwrap().unwrap(), "甲的正文");
        assert_eq!(load(&d, "乙.md").unwrap().unwrap(), "乙的正文");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 目标是非空目录（分组名可以叫 `笔记.md`）：绝不整棵删掉，源文件保持原样。
    #[test]
    fn move_path_refuses_non_empty_dir_target() {
        let d = tmp("refusedir");
        write(&d, "别的/笔记.md", "正文").unwrap();
        let from = sticker_path(&d, "别的/笔记.md").unwrap();
        let dir = sticker_path(&d, "笔记.md").unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("别动.txt"), b"keep").unwrap();

        let err = move_path(&from, &dir).unwrap_err().to_string();
        assert!(err.contains("不是空目录"), "错误信息应说明目录非空：{err}");
        assert_eq!(std::fs::read_to_string(from).unwrap(), "正文");
        assert_eq!(std::fs::read_to_string(dir.join("别动.txt")).unwrap(), "keep");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 目标是空目录（改名到已经建好的空分组文件夹）：允许，删掉空壳后搬进去。
    #[test]
    fn move_path_accepts_empty_dir_target() {
        let d = tmp("emptydir");
        write(&d, "旧名.md", "正文").unwrap();
        let from = sticker_path(&d, "旧名.md").unwrap();
        let to = d.join("stickers").join("随手记").join("新名.md");
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        assert!(!to.exists());
        std::fs::create_dir_all(&to).unwrap();
        move_path(&from, &to).unwrap();
        assert!(!from.exists());
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "正文");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 扩展名大小写不同（`笔记.MD` ↔ `笔记.md`）在 Windows 上是**同一个文件**：
    /// 必须当作"已经就位"直接成功，绝不能把源文件删掉。
    #[test]
    fn move_path_treats_case_only_difference_as_same_file() {
        let d = tmp("caseonly");
        write(&d, "笔记.md", "正文").unwrap();
        let from = sticker_path(&d, "笔记.md").unwrap();
        let to = sticker_path(&d, "笔记.MD").unwrap();
        move_path(&from, &to).unwrap();
        let live = if from.exists() { from } else { to };
        assert!(live.is_file(), "文件不能消失");
        assert_eq!(std::fs::read_to_string(&live).unwrap(), "正文");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 附件目录搬动用 `move_path_replacing`：目标是自己的旧目录时允许覆盖。
    #[test]
    fn move_path_replacing_overwrites_existing_asset_dir() {
        let d = tmp("replace");
        let from = d.join("assets").join("随手记").join("旧名_1");
        std::fs::create_dir_all(&from).unwrap();
        std::fs::write(from.join("a.png"), b"new").unwrap();
        let to = d.join("assets").join("随手记").join("新名_1");
        std::fs::create_dir_all(&to).unwrap();
        std::fs::write(to.join("stale.png"), b"old").unwrap();
        move_path_replacing(&from, &to).unwrap();
        assert!(!from.exists());
        assert_eq!(std::fs::read_to_string(to.join("a.png")).unwrap(), "new");
        assert!(!to.join("stale.png").exists(), "旧遗留文件应被替换掉");
        let _ = std::fs::remove_dir_all(&d);
    }
}
