//! 应用状态：内存缓存 + 数据库连接调度。
//!
//! `AppState` 由 Tauri `app.manage()` 托管，命令通过 `tauri::State` 访问；
//! 内部均为 `Arc`，可 `clone()` 后传入后台任务（调度器等）。
//! 数据库为单连接 `Arc<Mutex<Connection>>`，耗时操作应经
//! [`AppState::with_conn_async`] 派发到 `spawn_blocking`，避免阻塞 UI。

use std::sync::{Arc, Mutex, RwLock, RwLockReadGuard};

use anyhow::Result;
use rusqlite::Connection;

use crate::models::SystemConfig;

#[derive(Clone)]
pub struct AppState {
    conn: Arc<Mutex<Connection>>,
    config: Arc<RwLock<SystemConfig>>,
    db_path: Arc<String>,
}

impl AppState {
    pub fn new(conn: Connection, config: SystemConfig, db_path: String) -> Self {
        Self {
            conn: Arc::new(Mutex::new(conn)),
            config: Arc::new(RwLock::new(config)),
            db_path: Arc::new(db_path),
        }
    }

    /// 当前数据库文件路径（供健康检查/诊断用）。
    pub fn db_path(&self) -> &str {
        &self.db_path
    }

    /// 同步执行数据库闭包（调用方负责放在 spawn_blocking 中）。
    pub fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let guard = self
            .conn
            .lock()
            .map_err(|_| anyhow::anyhow!("数据库连接锁中毒"))?;
        f(&guard)
    }

    /// 异步执行数据库闭包：内部用 `tauri::async_runtime::spawn_blocking`。
    ///
    /// 注意：闭包内不要再嵌套 `with_conn`/`with_conn_async`，避免死锁。
    pub async fn with_conn_async<T, F>(&self, f: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&Connection) -> Result<T> + Send + 'static,
    {
        let conn = Arc::clone(&self.conn);
        tauri::async_runtime::spawn_blocking(move || {
            let guard = conn
                .lock()
                .map_err(|_| anyhow::anyhow!("数据库连接锁中毒"))?;
            f(&guard)
        })
        .await
        .map_err(|e| anyhow::anyhow!("spawn_blocking 失败: {e}"))?
    }

    /// 读取配置快照（只读）。
    pub fn config(&self) -> RwLockReadGuard<'_, SystemConfig> {
        self.config
            .read()
            .expect("SystemConfig 读锁中毒")
    }

    /// 重新加载 system_config 快照（配置变更后调用）。
    pub fn refresh_config(&self) -> Result<()> {
        let cfg = self.with_conn(crate::db::config_repo::load_all)?;
        let mut guard = self
            .config
            .write()
            .map_err(|_| anyhow::anyhow!("SystemConfig 写锁中毒"))?;
        *guard = cfg;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema;

    fn test_state() -> AppState {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        schema::run_migrations(&conn).unwrap();
        let config = SystemConfig::default();
        AppState::new(conn, config, ":memory:".to_string())
    }

    /// 多线程并发写：单连接 Mutex + 每线程独立 with_conn，无死锁且不丢数据。
    #[test]
    fn concurrent_writes_no_deadlock() {
        let state = test_state();
        let state = std::sync::Arc::new(state);

        let mut handles = Vec::new();
        for t in 0..8u32 {
            let state = state.clone();
            handles.push(std::thread::spawn(move || {
                for i in 0..5u32 {
                    state
                        .with_conn(|c| {
                            crate::db::sticker_repo::insert(
                                c,
                                &crate::db::sticker_repo::NewSticker {
                                    title: format!("t{t}-{i}"),
                                    ..Default::default()
                                },
                            )
                        })
                        .unwrap();
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }

        let count: i64 = state
            .with_conn(|c| {
                c.query_row("SELECT COUNT(*) FROM stickers", [], |r| r.get(0))
                    .map_err(|e| anyhow::anyhow!("{e}"))
            })
            .unwrap();
        assert_eq!(count, 40, "8 线程 × 5 条应全部写入");
    }
}
