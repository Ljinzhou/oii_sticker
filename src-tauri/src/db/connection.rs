//! SQLite 连接打开与默认路径管理。

use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::Connection;

/// 打开（首次运行则创建）数据库文件，启用外键与 WAL 模式。
/// 使用 `rusqlite` 的 `bundled` 特性，编译期内嵌 SQLite，目标机器无需
/// 安装任何系统级 SQLite 库。
pub fn open(db_path: &Path) -> Result<Connection> {
    // 确保父目录存在。
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建数据库父目录失败：{}", parent.display()))?;
    }
    let conn = Connection::open(db_path)
        .with_context(|| format!("打开数据库失败：{}", db_path.display()))?;

    // 应用平台级 PRAGMA：
    // - foreign_keys：开启外键约束（默认关闭，必须显式开启）。
    // - journal_mode = WAL：写日志模式，读写并发更友好。
    // - synchronous = NORMAL：与 WAL 配合，崩溃风险可接受，速度更快。
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;",
    )
    .context("应用数据库 PRAGMA 失败")?;

    Ok(conn)
}
