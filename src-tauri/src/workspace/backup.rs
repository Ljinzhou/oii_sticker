//! 备份与恢复。
//!
//! - `backup`：SQLite 在线快照（VACUUM INTO）+ zip 打包（排除 cache/）+ 写入 manifest.json。
//! - `inspect`：读取备份 zip 的签名与 manifest，校验必需条目（恢复前预览 / 校验）。
//! - `extract`：解压备份到目标目录（防目录穿越 + 必需条目校验），返回其签名。
//! - `transfer`：整目录复制（排除 cache/）→ 校验签名（删除源由命令层负责）。

use anyhow::{bail, Context, Result};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use super::layout::{read_signature, Layout, Signature};

/// zip 内固定条目名。
pub const ENTRY_SIGNATURE: &str = "workspace.json";
pub const ENTRY_DB: &str = "data/index.db";
pub const ENTRY_MANIFEST: &str = "manifest.json";

/// 备份格式版本：仅在语义变化时递增；新增字段不升版（读取端容忍缺失）。
pub const BACKUP_FORMAT: u32 = 1;

/// 备份元数据（zip 内 manifest.json）。旧版备份没有它 → inspect 返回 has_manifest=false。
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Manifest {
    pub format: u32,
    pub workspace_id: String,
    pub workspace_name: String,
    /// 备份时刻（Unix 毫秒；前端自行格式化，避免跨端时区/格式差异）。
    pub backup_at_ms: u64,
    pub app_version: String,
    pub schema_version: u32,
    /// zip 内条目数与未压缩字节数（不含 manifest 自身）。
    pub entries: u64,
    pub bytes: u64,
}

/// 备份文件信息（前端展示 / 恢复前确认）。
#[derive(serde::Serialize, Clone, Debug)]
pub struct BackupInfo {
    pub format: u32,
    pub has_manifest: bool,
    pub name: String,
    pub workspace_id: String,
    pub created_at: String,
    pub backup_at_ms: u64,
    pub app_version: String,
    pub schema_version: u32,
    pub entries: u64,
    pub bytes: u64,
    pub zip_bytes: u64,
}

/// 备份工作空间：SQLite 在线快照 + zip（排除 cache/）。
/// dest_zip 为输出 zip 完整路径；返回 zip 字节数。
pub fn backup(layout: &Layout, conn: &rusqlite::Connection, dest_zip: &Path) -> Result<u64> {
    // 备份文件不能落在工作空间内：否则打包时会读到正在写入的自身（自包含/损坏）
    let dest_abs = absolute(dest_zip)?;
    if dest_abs.starts_with(absolute(&layout.root)?) {
        bail!("备份文件不能保存在工作空间目录内，请换一个位置");
    }
    let sig = read_signature(&layout.signature_path())?.context("工作空间签名缺失，无法备份")?;
    // 快照落在 cache/（不参与打包）；目录被删时补建，避免 VACUUM INTO 失败
    let cache = layout.cache_dir();
    std::fs::create_dir_all(&cache).with_context(|| format!("创建缓存目录失败：{}", cache.display()))?;
    let snap = cache.join(format!("backup-snapshot-{}.db", unique_tag()));
    let _guard = TempFile(snap.clone());
    let snap_str = snap.to_string_lossy().replace('\\', "/");
    // 参数化绑定（VACUUM INTO ?1）：路径含单引号等字符不破坏 SQL
    conn.execute("VACUUM INTO ?1", [&snap_str]).context("生成数据库快照失败")?;

    let file = File::create(dest_zip).with_context(|| format!("创建备份文件失败：{}", dest_zip.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let mut entries = 0u64;
    let mut bytes = 0u64;
    // 1) 签名 + 快照 db（zip 内为 data/index.db）
    entries += 1;
    bytes += add_file(&mut zip, &layout.signature_path(), &layout.root, &options)?;
    entries += 1;
    bytes += add_file_named(&mut zip, ENTRY_DB, &snap, &options)?;
    // 2) stickers/** 与 assets/**、library/** 递归（目录缺失时跳过，不视为错误）
    for dir in [layout.stickers_dir(), layout.assets_dir(), layout.library_dir()] {
        for entry in walk_recursive(&dir)? {
            entries += 1;
            bytes += add_file(&mut zip, &entry, &layout.root, &options)?;
        }
    }
    let _ = std::fs::remove_file(&snap);

    // 3) manifest 最后写入（含条目数与字节数）
    let manifest = Manifest {
        format: BACKUP_FORMAT,
        workspace_id: sig.id.clone(),
        workspace_name: sig.name.clone(),
        backup_at_ms: now_ms(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        schema_version: schema_version(conn),
        entries,
        bytes,
    };
    zip.start_file(ENTRY_MANIFEST, options)
        .map_err(|e| anyhow::anyhow!("zip: {e}"))?;
    zip.write_all(serde_json::to_string_pretty(&manifest)?.as_bytes())?;
    let file = zip.finish().context("完成 zip 写入失败")?;
    let size = file.metadata().context("读取备份文件体积失败")?.len();

    // 4) 写后校验：能重新打开且关键条目齐全（损坏的备份不能留在磁盘上冒充成功）
    let info = inspect(dest_zip).context("备份文件校验失败")?;
    if info.entries == 0 || info.bytes == 0 {
        bail!("备份文件内容为空，请检查工作空间目录");
    }
    Ok(size)
}

/// 读取备份信息（前端预览 + 恢复前校验）。非本程序的备份返回错误。
pub fn inspect(zip_path: &Path) -> Result<BackupInfo> {
    let file = File::open(zip_path).with_context(|| format!("打开备份文件失败：{}", zip_path.display()))?;
    let zip_bytes = file.metadata().map(|m| m.len()).unwrap_or(0);
    let mut archive = zip::ZipArchive::new(file).context("读取备份失败：不是有效的 zip 文件")?;
    let sig: Signature = read_zip_json(&mut archive, ENTRY_SIGNATURE)?
        .context("备份缺少 workspace.json：不是本程序导出的工作空间备份")?;
    if archive.by_name(ENTRY_DB).is_err() {
        bail!("备份缺少 {ENTRY_DB}：备份文件不完整");
    }
    let manifest: Option<Manifest> = read_zip_json(&mut archive, ENTRY_MANIFEST)?;
    // 条目统计（manifest 缺失的旧备份按实际内容统计，忽略 manifest 自身）
    let mut entries = 0u64;
    let mut bytes = 0u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        if entry.name() == ENTRY_MANIFEST || entry.is_dir() {
            continue;
        }
        entries += 1;
        bytes += entry.size();
    }
    Ok(match manifest {
        Some(m) => BackupInfo {
            format: m.format,
            has_manifest: true,
            name: m.workspace_name,
            workspace_id: m.workspace_id,
            created_at: sig.created_at,
            backup_at_ms: m.backup_at_ms,
            app_version: m.app_version,
            schema_version: m.schema_version,
            entries: if m.entries > 0 { m.entries } else { entries },
            bytes: if m.bytes > 0 { m.bytes } else { bytes },
            zip_bytes,
        },
        None => BackupInfo {
            format: 0,
            has_manifest: false,
            name: sig.name,
            workspace_id: sig.id,
            created_at: sig.created_at,
            backup_at_ms: zip_bytes_mtime_ms(zip_path),
            app_version: String::new(),
            schema_version: 0,
            entries,
            bytes,
            zip_bytes,
        },
    })
}

/// 解压备份到 dest（需不存在或为空目录）；返回备份内的工作空间签名。
/// 防目录穿越：条目名必须是相对普通路径（zip slip）；manifest.json 不落盘。
pub fn extract(zip_path: &Path, dest: &Path) -> Result<Signature> {
    // 先整体校验，避免解压到一半才发现备份不完整
    let info = inspect(zip_path)?;
    let _ = info;
    let file = File::open(zip_path).with_context(|| format!("打开备份文件失败：{}", zip_path.display()))?;
    let mut archive = zip::ZipArchive::new(file).context("读取备份失败")?;
    std::fs::create_dir_all(dest).with_context(|| format!("创建目录失败：{}", dest.display()))?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        if name == ENTRY_MANIFEST || name.starts_with("cache/") || name == "cache" {
            continue; // 元数据与缓存都不落盘
        }
        let Some(rel) = entry.enclosed_name() else {
            bail!("备份包含非法路径条目：{name}");
        };
        let out = dest.join(&rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut writer = File::create(&out).with_context(|| format!("写入文件失败：{}", out.display()))?;
        std::io::copy(&mut entry, &mut writer).with_context(|| format!("解压文件失败：{name}"))?;
    }
    // 解压结果校验：签名可解析 + 数据库存在（与 inspect 的必需条目一致）
    let sig = read_signature(&dest.join(ENTRY_SIGNATURE))?.context("解压后缺少 workspace.json")?;
    if !dest.join(ENTRY_DB).is_file() {
        bail!("解压后缺少 {ENTRY_DB}，备份不完整");
    }
    Ok(sig)
}

/// 转移：整目录复制（跳过 cache/）→ 校验签名。
/// 只负责复制与校验；注册表更新、DB 切换、删源目录由命令层完成。
pub fn transfer(root: &Path, dest: &Path) -> Result<()> {
    if dest == root || dest.starts_with(root) {
        bail!("目标路径不能是源目录自身或其子目录");
    }
    copy_tree_excluding_cache(root, dest)?;
    // 校验：转移后签名必须存在且可解析。
    let sig = read_signature(&dest.join("workspace.json"))?.context("转移后签名缺失")?;
    let _ = sig;
    Ok(())
}

fn copy_tree_excluding_cache(src: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == "cache" {
            continue;
        }
        let target = dest.join(&name);
        if entry.path().is_dir() {
            copy_tree_excluding_cache(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// 递归列出目录内所有文件；目录不存在时返回空列表（备份不因缺目录失败）。
fn walk_recursive(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(dir)? {
        let p = entry?.path();
        if p.is_file() {
            out.push(p);
        } else if p.is_dir() {
            out.extend(walk_recursive(&p)?);
        }
    }
    out.sort();
    Ok(out)
}

/// 写入 zip 条目并返回写入字节数。
fn add_file<W: Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    path: &Path,
    root: &Path,
    options: &zip::write::SimpleFileOptions,
) -> Result<u64> {
    let rel = path.strip_prefix(root).unwrap_or(path);
    let rel = rel.to_string_lossy().replace('\\', "/");
    add_file_named(zip, &rel, path, options)
}

/// 以指定 zip 内路径写入文件（快照 db 需要以 data/index.db 为名）。
fn add_file_named<W: Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    entry_name: &str,
    path: &Path,
    options: &zip::write::SimpleFileOptions,
) -> Result<u64> {
    let bytes = std::fs::read(path).with_context(|| format!("读取文件失败：{}", path.display()))?;
    zip.start_file(entry_name.to_string(), *options).map_err(|e| anyhow::anyhow!("zip: {e}"))?;
    zip.write_all(&bytes)?;
    Ok(bytes.len() as u64)
}

fn read_zip_json<T: serde::de::DeserializeOwned>(
    archive: &mut zip::ZipArchive<File>,
    name: &str,
) -> Result<Option<T>> {
    let Ok(mut entry) = archive.by_name(name) else { return Ok(None) };
    let mut raw = String::new();
    entry.read_to_string(&mut raw).with_context(|| format!("读取 zip 内 {name} 失败"))?;
    Ok(Some(serde_json::from_str(&raw).with_context(|| format!("解析 zip 内 {name} 失败"))?))
}

fn schema_version(conn: &rusqlite::Connection) -> u32 {
    conn.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0)).unwrap_or(0)
}

/// 绝对化路径（尽量不依赖文件是否存在），用于「备份不能落在工作空间内」的判断。
fn absolute(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    Ok(std::env::current_dir().context("读取当前目录失败")?.join(path))
}

fn zip_bytes_mtime_ms(path: &Path) -> u64 {
    std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn unique_tag() -> String {
    format!("{}-{}", std::process::id(), now_ms())
}

/// 离开作用域即删除的临时文件（异常路径也不残留快照）。
struct TempFile(PathBuf);

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    fn temp_ws(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ws-backup-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// 小工作空间：签名 + 2 个便签 md + 1 个 asset（assets/12/）+ 1 个库文件 + cache 垃圾。
    fn build_ws(root: &Path) -> Layout {
        let layout = Layout::at(root);
        super::super::layout::ensure_layout(&layout, "备份测试").unwrap();
        std::fs::write(layout.stickers_dir().join("1-欢迎.md"), "# 欢迎\n内容").unwrap();
        std::fs::write(layout.stickers_dir().join("2-笔记.md"), "# 笔记").unwrap();
        std::fs::create_dir_all(layout.assets_dir().join("12")).unwrap();
        std::fs::write(layout.assets_dir().join("12").join("icon.png"), b"png-bytes").unwrap();
        std::fs::write(layout.library_dir().join("book.md"), "# 书").unwrap();
        std::fs::write(layout.cache_dir().join("scratch.bin"), b"cache-me-not").unwrap();
        let conn = rusqlite::Connection::open(layout.db_path()).unwrap();
        crate::db::schema::run_migrations(&conn).unwrap();
        let _ = conn.close();
        layout
    }

    fn archive_names(zip_path: &Path) -> Vec<String> {
        let f = File::open(zip_path).unwrap();
        let mut archive = zip::ZipArchive::new(f).unwrap();
        (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect()
    }

    fn backup_ws(layout: &Layout, zip_path: &Path) -> u64 {
        let conn = rusqlite::Connection::open(layout.db_path()).unwrap();
        let size = backup(layout, &conn, zip_path).unwrap();
        let _ = conn.close();
        size
    }

    #[test]
    fn backup_creates_zip_with_expected_entries() {
        let root = temp_ws("zip");
        let layout = build_ws(&root);
        let zip_path = root.parent().unwrap().join(format!("backup-{}.zip", std::process::id()));
        let _ = std::fs::remove_file(&zip_path);

        let size = backup_ws(&layout, &zip_path);

        assert!(zip_path.exists(), "zip 文件应生成");
        assert!(size > 0);
        let names = archive_names(&zip_path);
        assert!(names.contains(&ENTRY_SIGNATURE.to_string()), "签名应在 zip 内");
        assert!(names.contains(&ENTRY_DB.to_string()), "快照 db 应在 zip 内");
        assert!(names.contains(&ENTRY_MANIFEST.to_string()), "manifest 应在 zip 内");
        assert!(names.contains(&"stickers/1-欢迎.md".to_string()));
        assert!(names.contains(&"stickers/2-笔记.md".to_string()));
        assert!(names.contains(&"assets/12/icon.png".to_string()));
        assert!(names.contains(&"library/book.md".to_string()));
        assert!(
            !names.iter().any(|n| n.starts_with("cache/") || n.contains("backup-snapshot")),
            "cache/ 与快照临时文件不得进入 zip"
        );
        // 快照临时文件已清理
        assert!(
            std::fs::read_dir(layout.cache_dir())
                .unwrap()
                .all(|e| !e.unwrap().file_name().to_string_lossy().contains("backup-snapshot")),
            "cache/ 不得残留快照"
        );
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_file(&zip_path);
    }

    #[test]
    fn backup_zip_db_snapshot_is_readable_sqlite() {
        let root = temp_ws("snap");
        let layout = build_ws(&root);
        let zip_path = root.parent().unwrap().join(format!("snap-{}.zip", std::process::id()));
        let _ = std::fs::remove_file(&zip_path);
        backup_ws(&layout, &zip_path);

        let f = File::open(&zip_path).unwrap();
        let mut archive = zip::ZipArchive::new(f).unwrap();
        let mut buf = Vec::new();
        archive.by_name(ENTRY_DB).unwrap().read_to_end(&mut buf).unwrap();
        let tmp = root.join("check.db");
        std::fs::write(&tmp, &buf).unwrap();
        let conn = rusqlite::Connection::open(&tmp).unwrap();
        let v: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, crate::db::schema::SCHEMA_VERSION, "快照 db 应可打开且版本一致");
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_file(&zip_path);
    }

    #[test]
    fn backup_manifest_and_inspect_report_workspace_and_counts() {
        let root = temp_ws("manifest");
        let layout = build_ws(&root);
        let zip_path = root.parent().unwrap().join(format!("mani-{}.zip", std::process::id()));
        let _ = std::fs::remove_file(&zip_path);
        backup_ws(&layout, &zip_path);

        let info = inspect(&zip_path).unwrap();
        assert!(info.has_manifest, "新备份应带 manifest");
        assert_eq!(info.format, BACKUP_FORMAT);
        assert_eq!(info.name, "备份测试");
        assert!(!info.workspace_id.is_empty());
        assert!(info.backup_at_ms > 0);
        // 签名 + db + 2 md + 1 png + 1 库文件 = 6
        assert_eq!(info.entries, 6);
        assert!(info.bytes > 0);
        assert!(info.zip_bytes > 0);
        assert_eq!(info.schema_version, crate::db::schema::SCHEMA_VERSION);
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_file(&zip_path);
    }

    #[test]
    fn backup_rejects_dest_inside_workspace() {
        let root = temp_ws("inside");
        let layout = build_ws(&root);
        let inside = layout.cache_dir().join("self.zip");
        let conn = rusqlite::Connection::open(layout.db_path()).unwrap();
        let err = backup(&layout, &conn, &inside).unwrap_err().to_string();
        let _ = conn.close();
        assert!(err.contains("不能保存在工作空间目录内"), "实际：{err}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 工作空间缺少 stickers/assets/library 之一（用户手动删过）也必须能备份成功。
    #[test]
    fn backup_tolerates_missing_subdirs() {
        let root = temp_ws("missing");
        let layout = build_ws(&root);
        std::fs::remove_dir_all(layout.assets_dir()).unwrap();
        std::fs::remove_dir_all(layout.library_dir()).unwrap();
        let zip_path = root.parent().unwrap().join(format!("missing-{}.zip", std::process::id()));
        let _ = std::fs::remove_file(&zip_path);
        let size = backup_ws(&layout, &zip_path);
        assert!(size > 0);
        let names = archive_names(&zip_path);
        assert!(names.contains(&ENTRY_DB.to_string()));
        assert!(names.contains(&"stickers/1-欢迎.md".to_string()));
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_file(&zip_path);
    }

    #[test]
    fn inspect_rejects_non_backup_zip() {
        let dir = temp_ws("badzip");
        std::fs::create_dir_all(&dir).unwrap();
        let zip_path = dir.join("plain.zip");
        let file = File::create(&zip_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("readme.txt", options).unwrap();
        zip.write_all(b"hello").unwrap();
        zip.finish().unwrap();

        let err = inspect(&zip_path).unwrap_err().to_string();
        assert!(err.contains("不是本程序"), "实际：{err}");
        // 非 zip 文件
        let raw = dir.join("raw.bin");
        std::fs::write(&raw, b"not a zip").unwrap();
        assert!(inspect(&raw).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_restores_full_workspace_and_skips_manifest() {
        let root = temp_ws("extract");
        let layout = build_ws(&root);
        let zip_path = root.parent().unwrap().join(format!("ex-{}.zip", std::process::id()));
        let _ = std::fs::remove_file(&zip_path);
        backup_ws(&layout, &zip_path);

        let dest = root.parent().unwrap().join(format!("ex-dest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dest);
        let sig = extract(&zip_path, &dest).unwrap();
        assert_eq!(sig.name, "备份测试");
        assert!(dest.join(ENTRY_SIGNATURE).is_file());
        assert!(dest.join(ENTRY_DB).is_file());
        assert!(dest.join("stickers").join("1-欢迎.md").is_file());
        assert!(dest.join("assets").join("12").join("icon.png").is_file());
        assert!(dest.join("library").join("book.md").is_file());
        assert!(!dest.join(ENTRY_MANIFEST).exists(), "manifest 属 zip 元数据，不落盘");
        assert!(!dest.join("cache").exists(), "cache/ 不应被解压");
        // 解压出的库可直接打开
        let conn = rusqlite::Connection::open(dest.join(ENTRY_DB)).unwrap();
        let v: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, crate::db::schema::SCHEMA_VERSION);
        let _ = conn.close();
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&dest);
        let _ = std::fs::remove_file(&zip_path);
    }

    /// zip slip：条目名含 `..` 时拒绝解压（不得写到目标目录之外）。
    #[test]
    fn extract_rejects_path_traversal_entry() {
        let dir = temp_ws("slip");
        std::fs::create_dir_all(&dir).unwrap();
        // 手工造一个带穿越条目、但含必需文件的 zip
        let zip_path = dir.join("evil.zip");
        let file = File::create(&zip_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file(ENTRY_SIGNATURE, options).unwrap();
        zip.write_all(br#"{"id":"w-x","name":"evil","created_at":"0","last_used_at":"0"}"#).unwrap();
        zip.start_file(ENTRY_DB, options).unwrap();
        zip.write_all(b"db").unwrap();
        zip.start_file("../escape.txt", options).unwrap();
        zip.write_all(b"escaped").unwrap();
        zip.finish().unwrap();

        let dest = dir.join("out");
        let err = extract(&zip_path, &dest).unwrap_err().to_string();
        assert!(err.contains("非法路径"), "实际：{err}");
        assert!(!dir.join("escape.txt").exists(), "不得写到目标目录之外");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn transfer_copies_without_cache_src_untouched_signature_consistent() {
        let src = temp_ws("xfer-src");
        let layout = build_ws(&src);
        let dest = temp_ws("xfer-dest");

        transfer(&src, &dest).unwrap();
        // 源未删、未变
        assert!(src.join(ENTRY_SIGNATURE).exists());
        assert!(layout.stickers_dir().join("1-欢迎.md").exists());
        // 目标有一切，且无 cache
        assert!(dest.join(ENTRY_SIGNATURE).exists());
        assert!(dest.join("data").join("index.db").exists());
        assert!(dest.join("stickers").join("1-欢迎.md").exists());
        assert!(dest.join("stickers").join("2-笔记.md").exists());
        assert!(dest.join("assets").join("12").join("icon.png").exists());
        assert!(dest.join("library").join("book.md").exists());
        assert!(!dest.join("cache").exists(), "transfer 不得复制 cache/");
        // 签名一致
        let s1 = read_signature(&src.join(ENTRY_SIGNATURE)).unwrap().unwrap();
        let s2 = read_signature(&dest.join(ENTRY_SIGNATURE)).unwrap().unwrap();
        assert_eq!(s1.name, s2.name);
        assert_eq!(s1.id, s2.id);
        let _ = std::fs::remove_dir_all(&src);
        let _ = std::fs::remove_dir_all(&dest);
    }

    #[test]
    fn transfer_rejects_dest_inside_src() {
        let src = temp_ws("bad-dest");
        build_ws(&src);
        assert!(transfer(&src, &src.join("nested").join("cp")).is_err());
        let _ = std::fs::remove_dir_all(&src);
    }

    /// 路径含单引号（如 "Bob's ws"）时备份必须成功：
    /// VACUUM INTO 参数化绑定后不再受 SQL 字符串转义影响。
    #[test]
    fn backup_works_when_snapshot_path_contains_single_quote() {
        let base = std::env::temp_dir().join(format!("ws-backup-quote-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("Bob's workspace");
        let layout = build_ws(&root);
        let zip_path = base.join("backup'rest.zip");
        let _ = std::fs::remove_file(&zip_path);

        let size = backup_ws(&layout, &zip_path);

        assert!(size > 0, "含引号路径备份应成功");
        assert!(zip_path.exists());
        let names = archive_names(&zip_path);
        assert!(names.contains(&ENTRY_DB.to_string()));
        let _ = std::fs::remove_dir_all(&base);
        let _ = std::fs::remove_file(&zip_path);
    }
}
