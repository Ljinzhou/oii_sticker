//! 工作空间磁盘布局与注册表。
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const DEFAULT_ROOT_NAME: &str = "oiistiker_workspace";
const BAD_CHARS: &[char] = &['\\', '/', ':', '*', '?', '"', '<', '>', '|', '%', '&', '{', '}', '$', '\'', '@', '+', '`', '=', ';', ',', '#'];

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
    pub fn stickers_dir(&self) -> PathBuf { self.root.join("stickers") }
    pub fn assets_dir(&self) -> PathBuf { self.root.join("assets") }
    pub fn library_dir(&self) -> PathBuf { self.root.join("library") }
    pub fn cache_dir(&self) -> PathBuf { self.root.join("cache") }
    pub fn signature_path(&self) -> PathBuf { self.root.join("workspace.json") }
}

/// 从当前 DB 文件路径推算工作空间根目录（db 位于 <root>/data/index.db）。
pub fn root_from_db_path(db_path: &Path) -> Option<PathBuf> {
    db_path.parent()?.parent().map(Path::to_path_buf)
}

/// 创建完整目录结构 + 写入可读签名 workspace.json。
pub fn ensure_layout(layout: &Layout, name: &str) -> Result<()> {
    for dir in [layout.stickers_dir(), layout.assets_dir(), layout.library_dir(), layout.cache_dir()] {
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

pub fn read_signature(path: &Path) -> Result<Option<Signature>> {
    if !path.exists() { return Ok(None); }
    let raw = std::fs::read_to_string(path).context("读取 workspace.json 失败")?;
    Ok(Some(serde_json::from_str(&raw).context("解析 workspace.json 失败")?))
}

/// 文件名清洗：替换非法字符，限制长度，保留中文与常见符号。
pub fn sanitize_file_name(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| if BAD_CHARS.contains(&c) { '-' } else { c })
        .take(80)
        .collect();
    let trimmed = cleaned.trim().trim_matches('-');
    let trimmed = if trimmed.is_empty() || trimmed.chars().all(|c| c == '.') { "未命名" } else { trimmed };
    trimmed.to_string()
}

pub fn sticker_file_name(id: i64, title: &str) -> String {
    format!("{id}-{}.md", sanitize_file_name(title))
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

    #[test]
    fn sanitize_keeps_chinese_and_replaces_bad_chars() {
        assert_eq!(sanitize_file_name("我的 笔记/啊？"), "我的 笔记-啊？");
        assert_eq!(sanitize_file_name(".."), "未命名");
        assert_eq!(sanitize_file_name("a").len(), 1);
    }

    #[test]
    fn sticker_file_name_uses_id_and_title() {
        assert_eq!(sticker_file_name(12, "欢迎使用"), "12-欢迎使用.md");
    }

    #[test]
    fn ensure_layout_creates_all_dirs_and_signature() {
        let tmp = std::env::temp_dir().join(format!("ws-layout-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let layout = Layout::at(&tmp);
        ensure_layout(&layout, "默认值").unwrap();
        assert!(layout.db_path().parent().unwrap().is_dir());
        assert!(layout.stickers_dir().is_dir());
        assert!(layout.assets_dir().is_dir());
        assert!(layout.library_dir().is_dir());
        assert!(layout.cache_dir().is_dir());
        let sig = read_signature(&layout.signature_path()).unwrap().unwrap();
        assert_eq!(sig.name, "默认值");
        let _ = std::fs::remove_dir_all(&tmp);
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
