//! 工作空间磁盘布局与注册表。
//!
//! 工作空间里只有四个顶层项，别无其他：
//! - `stickers/` —— 便签正文（主存储），**按分组分目录**：`stickers/<分组…>/<标题>.md`
//! - `assets/`   —— 附件（粘贴图片等）：`assets/<分组…>/<标题>_<id>/<文件>`
//! - `data/`     —— SQLite（`data/index.db`）
//! - `workspace.json` —— 可读签名
//!
//! 历史版本还会创建 `cache/` 与 `library/`，现已移除（见 `staging_dir`）：
//! 临时文件一律放到工作空间**之外**的同卷兄弟目录。
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::naming;

pub const DEFAULT_ROOT_NAME: &str = "oiistiker_workspace";
/// 暂存目录名前缀（位于工作空间同级，见 [`Layout::staging_dir`]）。
pub const STAGING_PREFIX: &str = ".oiisticker-tmp-";

/// 工作空间目录布局（root = 工作空间根目录）。
#[derive(Clone, Debug)]
pub struct Layout {
    pub root: PathBuf,
}

impl Layout {
    pub fn at(root: &Path) -> Self {
        Self { root: root.to_path_buf() }
    }
    pub fn db_path(&self) -> PathBuf { self.root.join("data").join("index.db") }
    /// 便签 md 根目录（分组即其下的子目录）。
    pub fn stickers_dir(&self) -> PathBuf { self.root.join("stickers") }
    /// 附件根目录（分组即其下的子目录）。
    pub fn assets_dir(&self) -> PathBuf { self.root.join("assets") }
    pub fn signature_path(&self) -> PathBuf { self.root.join("workspace.json") }

    /// 暂存目录：工作空间**之外**的同卷兄弟目录 `<parent>/.oiisticker-tmp-<工作空间名>/`。
    ///
    /// 备份快照、恢复 staging、回滚点都放这里，理由三条：
    /// - 工作空间里不该出现 `cache/` 这类程序目录（用户看到的就是他的文件）；
    /// - 与工作空间同卷，`std::fs::rename` 仍是原子改名（跨卷 rename 会失败），
    ///   覆盖式恢复"旧数据改名保存 + 新数据改名就位"的安全语义才能保住；
    /// - 覆盖恢复会把整份 `data/` 换掉，暂存目录绝不能放在 `data/` 里。
    pub fn staging_dir(&self) -> PathBuf {
        let name = self
            .root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "workspace".to_string());
        match self.root.parent() {
            // 盘符根目录（`E:\`）没有父目录，只能退化为放工作空间内
            Some(parent) => parent.join(format!("{STAGING_PREFIX}{name}")),
            None => self.root.join(format!("{STAGING_PREFIX}workspace")),
        }
    }
}

/// 从当前 DB 文件路径推算工作空间根目录（db 位于 <root>/data/index.db）。
pub fn root_from_db_path(db_path: &Path) -> Option<PathBuf> {
    db_path.parent()?.parent().map(Path::to_path_buf)
}

/// 便签的 md 文件名（标题 + `.md`，按系统命名规则校验）。
pub fn sticker_md_name(title: &str) -> Result<String> {
    naming::sticker_md_name(title)
}

/// 创建目录结构 + 写入可读签名 workspace.json。
///
/// 只建 `stickers/`、`assets/`、`data/`：不再创建 `cache/`、`library/`。
pub fn ensure_layout(layout: &Layout, name: &str) -> Result<()> {
    for dir in [layout.stickers_dir(), layout.assets_dir()] {
        std::fs::create_dir_all(&dir).with_context(|| format!("创建目录失败：{}", dir.display()))?;
    }
    std::fs::create_dir_all(layout.db_path().parent().unwrap()).context("创建 data 目录失败")?;
    if !layout.signature_path().exists() {
        let sig = Signature {
            id: format!("w-{}", chrono_now_hex()),
            name: name.to_string(),
            created_at: now_iso(),
            last_used_at: now_iso(),
        };
        atomic_write_json(&layout.signature_path(), &sig)?;
    }
    Ok(())
}

/// 清掉历史版本留在工作空间里的程序目录（`cache/`、`library/`）。
///
/// 新版本不再创建它们；对已有工作空间，用户已确认「都直接删除」。
/// 返回被删除的目录名，供调用方记录日志。
pub fn remove_legacy_dirs(layout: &Layout) -> Vec<String> {
    let mut removed = Vec::new();
    for name in ["cache", "library"] {
        let dir = layout.root.join(name);
        if dir.is_dir() && std::fs::remove_dir_all(&dir).is_ok() {
            removed.push(name.to_string());
        }
    }
    removed
}

pub fn read_signature(path: &Path) -> Result<Option<Signature>> {
    if !path.exists() { return Ok(None); }
    let raw = std::fs::read_to_string(path).context("读取 workspace.json 失败")?;
    Ok(Some(serde_json::from_str(&raw).context("解析 workspace.json 失败")?))
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Signature {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub last_used_at: String,
}

// ── 注册表（程序级，位于 app_data_dir/workspaces.json）──
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Registry {
    pub current: Option<String>,
    pub workspaces: Vec<WorkspaceEntry>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WorkspaceEntry {
    pub id: String,
    pub name: String,
    pub path: String,
    pub created_at: String,
}

pub fn load_registry(path: &Path) -> Result<Registry> {
    if !path.exists() { return Ok(Registry::default()); }
    let raw = std::fs::read_to_string(path).context("读取注册表失败")?;
    Ok(serde_json::from_str(&raw).context("解析注册表失败")?)
}

pub fn save_registry(path: &Path, registry: &Registry) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("创建注册表目录失败")?;
    }
    atomic_write_json(path, registry)
}

pub fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    atomic_write(path, serde_json::to_string_pretty(value)?.as_bytes())
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建目录失败：{}", parent.display()))?;
    }
    naming::ensure_path_fits(path)?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).with_context(|| format!("写临时文件失败：{}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("重命名失败：{}", path.display()))?;
    Ok(())
}

fn now_iso() -> String { time_at_utc_now() }
fn chrono_now_hex() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let millis = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
    format!("{millis:x}")
}
fn time_at_utc_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    format!("{secs}")
}

/// 默认位置：用户文档目录/oiistiker_workspace。
pub fn default_root() -> Option<PathBuf> {
    dirs::document_dir().map(|d| d.join(DEFAULT_ROOT_NAME))
}

/// 目录选择校验的错误前缀：目标文件夹已存在且非空。
/// 前端据此弹确认「是否在其中创建 oiistiker_workspace 子目录」。
pub const ERR_DEST_NOT_EMPTY: &str = "DEST_NOT_EMPTY:";

/// 校验工作控件目标目录：
/// - 不存在 → 创建（含父目录）并返回 Ok；
/// - 存在且为空目录 → Ok；
/// - 存在且非空 → Err，消息以 [`ERR_DEST_NOT_EMPTY`] 前缀开头；
/// - 路径存在但不是文件夹 → Err。
pub fn ensure_empty_dest(dest: &Path) -> Result<()> {
    if !dest.exists() {
        std::fs::create_dir_all(dest)
            .with_context(|| format!("创建目录失败：{}", dest.display()))?;
        return Ok(());
    }
    if !dest.is_dir() {
        bail!("目标路径不是文件夹：{}", dest.display());
    }
    let is_empty = std::fs::read_dir(dest)?.next().is_none();
    if !is_empty {
        bail!("{ERR_DEST_NOT_EMPTY}{}", dest.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_root(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("ws-layout-{tag}-{}", std::process::id()))
    }

    #[test]
    fn ensure_layout_creates_only_three_dirs_and_signature() {
        let tmp = tmp_root("dirs");
        let _ = std::fs::remove_dir_all(&tmp);
        let layout = Layout::at(&tmp);
        ensure_layout(&layout, "默认值").unwrap();
        assert!(layout.db_path().parent().unwrap().is_dir());
        assert!(layout.stickers_dir().is_dir());
        assert!(layout.assets_dir().is_dir());
        // cache/ 与 library/ 不再创建
        assert!(!tmp.join("cache").exists());
        assert!(!tmp.join("library").exists());
        let sig = read_signature(&layout.signature_path()).unwrap().unwrap();
        assert_eq!(sig.name, "默认值");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn remove_legacy_dirs_deletes_cache_and_library_only() {
        let tmp = tmp_root("legacy");
        let _ = std::fs::remove_dir_all(&tmp);
        let layout = Layout::at(&tmp);
        ensure_layout(&layout, "旧目录").unwrap();
        std::fs::create_dir_all(tmp.join("cache")).unwrap();
        std::fs::create_dir_all(tmp.join("library")).unwrap();
        std::fs::write(tmp.join("library").join("book.md"), b"x").unwrap();
        assert!(tmp.join("stickers").is_dir());
        let removed = remove_legacy_dirs(&layout);
        assert_eq!(removed.len(), 2, "{removed:?}");
        assert!(!tmp.join("cache").exists());
        assert!(!tmp.join("library").exists());
        assert!(tmp.join("stickers").is_dir(), "正常目录不受影响");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn staging_dir_is_outside_workspace_and_on_same_volume() {
        let root = if cfg!(windows) { PathBuf::from(r"C:\ws-test\我的文档") } else { PathBuf::from("/ws-test/我的文档") };
        let layout = Layout::at(&root);
        let staging = layout.staging_dir();
        assert!(!staging.starts_with(&root), "暂存目录必须在工作空间之外：{}", staging.display());
        assert_eq!(staging.parent(), root.parent(), "必须与工作空间同卷（同父目录）");
        assert!(staging.file_name().unwrap().to_string_lossy().starts_with(STAGING_PREFIX));
        assert!(!staging.starts_with(layout.stickers_dir()));
    }

    #[test]
    fn sticker_md_name_uses_title_only() {
        assert_eq!(sticker_md_name("欢迎使用").unwrap(), "欢迎使用.md");
        assert!(sticker_md_name("带/斜杠").is_err());
    }

    #[test]
    fn registry_roundtrip_and_default_when_missing() {
        let tmp = std::env::temp_dir().join(format!("ws-reg-test-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&tmp);
        assert!(load_registry(&tmp).unwrap().workspaces.is_empty());
        let mut reg = Registry { current: Some("w-1".into()), workspaces: vec![WorkspaceEntry { id: "w-1".into(), name: "工作区一".into(), path: "C:/ws".into(), created_at: "1".into() }] };
        save_registry(&tmp, &reg).unwrap();
        let loaded = load_registry(&tmp).unwrap();
        assert_eq!(loaded.current.as_deref(), Some("w-1"));
        assert_eq!(loaded.workspaces.len(), 1);
        let _ = std::fs::remove_file(&tmp);
        reg.workspaces.clear();
        assert!(reg.workspaces.is_empty());
    }

    #[test]
    fn ensure_empty_dest_accepts_missing_and_empty_rejects_nonempty() {
        let tmp = std::env::temp_dir().join(format!("ws-empty-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        // 不存在 → 创建
        ensure_empty_dest(&tmp).unwrap();
        assert!(tmp.is_dir());
        // 空目录 → Ok
        ensure_empty_dest(&tmp).unwrap();
        // 非空 → 前缀错误
        std::fs::write(tmp.join("占用.txt"), b"x").unwrap();
        let err = ensure_empty_dest(&tmp).unwrap_err().to_string();
        assert!(err.starts_with(ERR_DEST_NOT_EMPTY), "实际：{err}");
        // 文件路径 → 拒绝
        let file = tmp.join("文件.md");
        std::fs::write(&file, b"x").unwrap();
        assert!(ensure_empty_dest(&file).is_err());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
