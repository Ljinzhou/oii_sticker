//! `asset_repo` 提供对 `assets` 表的 CRUD。
//! 源项目缺失该 repo（assets 表为死表），本工程补齐基础能力。

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

use crate::models::Asset;

/// 新建资源条目，返回自增 id。
pub fn insert(
    conn: &Connection,
    sticker_id: Option<i64>,
    name: &str,
    mime_type: &str,
    file_path: &str,
    file_size: i64,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO assets (sticker_id, name, mime_type, file_path, file_size)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![sticker_id, name, mime_type, file_path, file_size],
    )
    .context("插入 asset 失败")?;
    Ok(conn.last_insert_rowid())
}

/// 按 id 读取资源。
pub fn get(conn: &Connection, id: i64) -> Result<Option<Asset>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, sticker_id, name, mime_type, file_path, file_size, created_at
           FROM assets WHERE id = ?1",
    )?;
    let r = stmt.query_row(params![id], row_to_asset).optional()?;
    Ok(r)
}

/// 列出某个便签的所有资源。
pub fn list_by_sticker(conn: &Connection, sticker_id: i64) -> Result<Vec<Asset>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, sticker_id, name, mime_type, file_path, file_size, created_at
           FROM assets WHERE sticker_id = ?1 ORDER BY id",
    )?;
    let rows = stmt
        .query_map(params![sticker_id], row_to_asset)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// 删除资源条目（不负责删除磁盘文件）。
pub fn delete(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM assets WHERE id = ?1", params![id])
        .context("删除 asset 失败")?;
    Ok(())
}

fn row_to_asset(row: &rusqlite::Row<'_>) -> rusqlite::Result<Asset> {
    Ok(Asset {
        id: row.get(0)?,
        sticker_id: row.get(1)?,
        name: row.get(2)?,
        mime_type: row.get(3)?,
        file_path: row.get(4)?,
        file_size: row.get(5)?,
        created_at: row.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{schema, sticker_repo};

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        schema::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn asset_crud() {
        let conn = test_conn();
        let sid = sticker_repo::insert(
            &conn,
            &sticker_repo::NewSticker {
                title: "资源宿主".into(),
                ..Default::default()
            },
        )
        .unwrap();

        let aid = insert(&conn, Some(sid), "a.png", "image/png", "assets/a.png", 1234).unwrap();
        let a = get(&conn, aid).unwrap().unwrap();
        assert_eq!(a.name, "a.png");
        assert_eq!(a.file_size, 1234);

        let list = list_by_sticker(&conn, sid).unwrap();
        assert_eq!(list.len(), 1);

        delete(&conn, aid).unwrap();
        assert!(get(&conn, aid).unwrap().is_none());
    }
}
