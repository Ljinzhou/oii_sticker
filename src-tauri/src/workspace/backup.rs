//! 备份与转移。
//!
//! - `backup`：SQLite 在线快照（VACUUM INTO）+ zip 打包（排除 cache/）。
//! - `transfer`：整目录复制（排除 cache/）→ 校验签名（删除源由命令层负责）。

use anyhow::{bail, Context, Result};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::layout::{ensure_layout, read_signature, Layout};

/// 备份工作控件：SQLite 在线快照 + zip（排除 cache/）。
/// dest_zip 为输出 zip 完整路径；返回 zip 字节数。
pub fn backup(layout: &Layout, conn: &rusqlite::Connection, dest_zip: &Path) -> Result<u64> {
    let snap = layout.cache_dir().join("backup-snapshot.db");
    let _ = std::fs::remove_file(&snap);
    let snap_str = snap.to_string_lossy().replace('\\', "/");
    conn.execute(&format!("VACUUM INTO '{snap_str}'"), [])
        .context("生成数据库快照失败")?;
    let file = File::create(dest_zip).with_context(|| format!("创建备份文件失败：{}", dest_zip.display()))?;
    let mut zip = zip::ZipWriter::new(file);    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    // 1) 签名 + 快照 db（zip 内为 data/index.db）
    add_file(&mut zip, &layout.signature_path(), &layout.root, &options)?;
    add_file_named(&mut zip, "data/index.db", &snap, &options)?;
    // 2) stickers/** 与 assets/**、library/** 递归
    for entry in walk_recursive(&layout.stickers_dir())? {
        add_file(&mut zip, &entry, &layout.root, &options)?;
    }
    for entry in walk_recursive(&layout.assets_dir())? {
        add_file(&mut zip, &entry, &layout.root, &options)?;
    }
    for entry in walk_recursive(&layout.library_dir())? {
        add_file(&mut zip, &entry, &layout.root, &options)?;
    }
    let _ = std::fs::remove_file(&snap);
    let file = zip.finish().context("完成 zip 写入失败")?;
    let size = file.metadata().context("读取备份文件体积失败")?.len();
    Ok(size)
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

fn walk_recursive(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
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

fn add_file<W: Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    path: &Path,
    root: &Path,
    options: &zip::write::SimpleFileOptions,
) -> Result<()> {
    let rel = path.strip_prefix(root).unwrap();
    let rel = rel.to_string_lossy().replace('\\', "/");
    zip.start_file(rel, *options).map_err(|e| anyhow::anyhow!("zip: {e}"))?;
    zip.write_all(&std::fs::read(path)?)?;
    Ok(())
}

/// 以指定 zip 内路径写入文件（快照 db 需要以 data/index.db 为名）。
fn add_file_named<W: Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    entry_name: &str,
    path: &Path,
    options: &zip::write::SimpleFileOptions,
) -> Result<()> {
    zip.start_file(entry_name.to_string(), *options).map_err(|e| anyhow::anyhow!("zip: {e}"))?;
    zip.write_all(&std::fs::read(path)?)?;
    Ok(())
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

    /// 小工作控件：签名 + 2 个便签 md + 1 个 asset（assets/12/）+ 1 个库文件 + cache 垃圾。
    fn build_ws(root: &Path) -> Layout {
        let layout = Layout::at(root);
        ensure_layout(&layout, "备份测试").unwrap();
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

    #[test]
    fn backup_creates_zip_with_expected_entries() {
        let root = temp_ws("zip");
        let layout = build_ws(&root);
        let zip_path = root.parent().unwrap().join(format!("backup-{}.zip", std::process::id()));
        let _ = std::fs::remove_file(&zip_path);

        let conn = rusqlite::Connection::open(layout.db_path()).unwrap();
        let size = backup(&layout, &conn, &zip_path).unwrap();
        let _ = conn.close();

        assert!(zip_path.exists(), "zip 文件应生成");
        assert!(size > 0);
        let names = archive_names(&zip_path);
        assert!(names.contains(&"workspace.json".to_string()), "签名应在 zip 内");
        assert!(names.contains(&"data/index.db".to_string()), "快照 db 应在 zip 内");
        assert!(names.contains(&"stickers/1-欢迎.md".to_string()));
        assert!(names.contains(&"stickers/2-笔记.md".to_string()));
        assert!(names.contains(&"assets/12/icon.png".to_string()));
        assert!(names.contains(&"library/book.md".to_string()));
        assert!(
            !names.iter().any(|n| n.starts_with("cache/") || n.contains("backup-snapshot")),
            "cache/ 与快照临时文件不得进入 zip"
        );
        // 快照临时文件已清理
        assert!(!layout.cache_dir().join("backup-snapshot.db").exists());
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_file(&zip_path);
    }

    #[test]
    fn backup_zip_db_snapshot_is_readable_sqlite() {
        let root = temp_ws("snap");
        let layout = build_ws(&root);
        let zip_path = root.parent().unwrap().join(format!("snap-{}.zip", std::process::id()));
        let _ = std::fs::remove_file(&zip_path);
        let conn = rusqlite::Connection::open(layout.db_path()).unwrap();
        backup(&layout, &conn, &zip_path).unwrap();
        let _ = conn.close();

        let f = File::open(&zip_path).unwrap();
        let mut archive = zip::ZipArchive::new(f).unwrap();
        let mut buf = Vec::new();
        use std::io::Read;
        archive.by_name("data/index.db").unwrap().read_to_end(&mut buf).unwrap();
        let tmp = root.join("check.db");
        std::fs::write(&tmp, &buf).unwrap();
        let conn = rusqlite::Connection::open(&tmp).unwrap();
        let v: u32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, crate::db::schema::SCHEMA_VERSION, "快照 db 应可打开且版本一致");
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_file(&zip_path);
    }

    #[test]
    fn transfer_copies_without_cache_src_untouched_signature_consistent() {
        let src = temp_ws("xfer-src");
        let layout = build_ws(&src);
        let dest = temp_ws("xfer-dest");

        transfer(&src, &dest).unwrap();
        // 源未删、未变
        assert!(src.join("workspace.json").exists());
        assert!(layout.stickers_dir().join("1-欢迎.md").exists());
        // 目标有一切，且无 cache
        assert!(dest.join("workspace.json").exists());
        assert!(dest.join("data").join("index.db").exists());
        assert!(dest.join("stickers").join("1-欢迎.md").exists());
        assert!(dest.join("stickers").join("2-笔记.md").exists());
        assert!(dest.join("assets").join("12").join("icon.png").exists());
        assert!(dest.join("library").join("book.md").exists());
        assert!(!dest.join("cache").exists(), "transfer 不得复制 cache/");
        // 签名一致
        let s1 = read_signature(&src.join("workspace.json")).unwrap().unwrap();
        let s2 = read_signature(&dest.join("workspace.json")).unwrap().unwrap();
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
}
