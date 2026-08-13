// 数据层/repo 由阶段 3 的命令层接入前，先允许 dead_code（避免 40+ 噪音警告）；
// 阶段 3 完成命令接入后移除该属性。
#![allow(dead_code)]

use serde::Serialize;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

mod db;
mod models;
mod state;

use state::AppState;

/// 数据库健康检查：返回 schema 版本、表清单与路径（供前端验证）。
#[derive(Serialize)]
struct DbHealth {
    user_version: u32,
    tables: Vec<String>,
    db_path: String,
    config_keys: usize,
}

#[tauri::command]
fn db_health(state: tauri::State<'_, AppState>) -> Result<DbHealth, String> {
    state
        .with_conn(|conn| {
            let user_version: u32 = conn
                .query_row("PRAGMA user_version", [], |r| r.get(0))
                .map_err(|e| anyhow::anyhow!("读取 user_version 失败: {e}"))?;
            let tables: Vec<String> = conn
                .prepare(
                    "SELECT name FROM sqlite_master
                      WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
                )
                .map_err(|e| anyhow::anyhow!("读取表清单失败: {e}"))?
                .query_map([], |r| r.get(0))
                .map_err(|e| anyhow::anyhow!("读取表清单失败: {e}"))?
                .collect::<rusqlite::Result<_>>()
                .map_err(|e| anyhow::anyhow!("读取表清单失败: {e}"))?;
            Ok(DbHealth {
                user_version,
                tables,
                db_path: state.db_path().to_string(),
                config_keys: state.config().entries.len(),
            })
        })
        .map_err(|e| e.to_string())
}

/// 创建一个独立便签窗口（透明、无边框、不出现在任务栏）。
/// 返回窗口 label（形如 `sticker-<n>`，n 自动递增避免冲突）。
#[tauri::command]
fn create_sticker_window(
    app: tauri::AppHandle,
    title: String,
    x: f64,
    y: f64,
) -> Result<String, String> {
    let mut n = 1;
    let label = loop {
        let candidate = format!("sticker-{n}");
        if app.get_webview_window(&candidate).is_none() {
            break candidate;
        }
        n += 1;
    };

    WebviewWindowBuilder::new(&app, &label, WebviewUrl::App("index.html".into()))
        .title(title)
        .inner_size(320.0, 240.0)
        .position(x, y)
        .transparent(true)
        .decorations(false)
        .skip_taskbar(true)
        .resizable(true)
        .build()
        .map_err(|e| e.to_string())?;

    Ok(label)
}

/// 列出当前所有便签窗口的 label。
#[tauri::command]
fn list_sticker_windows(app: tauri::AppHandle) -> Vec<String> {
    app.webview_windows()
        .keys()
        .filter(|l| l.starts_with("sticker-"))
        .cloned()
        .collect()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            // 数据层初始化：app_data_dir/stickers.db + 迁移 + 配置快照
            let db_path = app
                .path()
                .app_data_dir()
                .map_err(|e| format!("获取 app_data_dir 失败: {e}"))?
                .join("stickers.db");
            let conn = db::connection::open(&db_path)
                .map_err(|e| format!("打开数据库失败: {e}"))?;
            db::schema::run_migrations(&conn).map_err(|e| format!("数据库迁移失败: {e}"))?;
            let config = db::config_repo::load_all(&conn)
                .map_err(|e| format!("读取配置失败: {e}"))?;
            app.manage(AppState::new(
                conn,
                config,
                db_path.to_string_lossy().into_owned(),
            ));

            // 验证 Demo：启动时自动创建 3 个演示便签窗口（位置错开），
            // 便于无人工交互即可验证多窗口 + 背景半透明效果。
            // 背景色由前端按窗口 label 从调色板取（见 StickerWindow.vue）。
            for i in 0..3 {
                let label = format!("sticker-{}", i + 1);
                let _ = WebviewWindowBuilder::new(
                    app,
                    &label,
                    WebviewUrl::App("index.html".into()),
                )
                .title(format!("便签 {}", i + 1))
                .inner_size(320.0, 240.0)
                .position(180.0 + i as f64 * 70.0, 100.0 + i as f64 * 70.0)
                .transparent(true)
                .decorations(false)
                .skip_taskbar(true)
                .resizable(true)
                .build();
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            create_sticker_window,
            list_sticker_windows,
            db_health
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
