//! `config_repo` 提供对 `system_config` 表的读写。

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

use crate::models::{ConfigEntry, SystemConfig};

/// 读取全部配置行，组装成 `SystemConfig` 快照。
pub fn load_all(conn: &Connection) -> Result<SystemConfig> {
    let mut stmt = conn.prepare_cached(
        "SELECT key, value, description, updated_at FROM system_config",
    )?;
    let entries = stmt
        .query_map([], |row| {
            Ok(ConfigEntry {
                key: row.get(0)?,
                value: row.get(1)?,
                description: row.get(2)?,
                updated_at: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut map = std::collections::HashMap::new();
    for e in entries {
        map.insert(e.key.clone(), e.value);
    }
    Ok(SystemConfig { entries: map })
}

/// 按 key 读单条。
pub fn get(conn: &Connection, key: &str) -> Result<Option<String>> {
    let v = conn
        .query_row(
            "SELECT value FROM system_config WHERE key = ?1",
            params![key],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    Ok(v)
}

/// 写入或覆盖单条配置。
pub fn set(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO system_config (key, value, description) VALUES (?1, ?2, '')
         ON CONFLICT(key) DO UPDATE SET
            value = excluded.value,
            updated_at = datetime('now')",
        params![key, value],
    )
    .context("写入 system_config 失败")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        schema::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn config_set_get_roundtrip() {
        let conn = test_conn();
        set(&conn, "my_key", "my_value").unwrap();
        assert_eq!(get(&conn, "my_key").unwrap().as_deref(), Some("my_value"));

        // 覆盖
        set(&conn, "my_key", "new_value").unwrap();
        assert_eq!(get(&conn, "my_key").unwrap().as_deref(), Some("new_value"));

        // 不存在
        assert!(get(&conn, "nope").unwrap().is_none());
    }

    #[test]
    fn load_all_includes_defaults() {
        let conn = test_conn();
        let cfg = load_all(&conn).unwrap();
        assert_eq!(cfg.get_or("default_sticker_opacity", ""), "0.9");
        assert_eq!(cfg.get_or("default_sticker_bg_color", ""), "#CCFFCC");
        assert!(cfg.entries.len() >= 15);
    }
}
