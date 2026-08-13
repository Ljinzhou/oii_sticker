// 数据层/repo 由阶段 3 的命令层接入前，先允许 dead_code（避免 40+ 噪音警告）；
// 阶段 3 完成命令接入后移除该属性。
#![allow(dead_code)]

use serde::Serialize;
use tauri::{Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

mod commands;
mod db;
mod datetime;
mod editing;
mod events;
mod models;
mod platform;
mod reminder;
mod slash;
mod state;

use commands::create_sticker;
use db::sticker_repo::NewSticker;
use platform::tray::TrayAction;
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
/// 返回窗口 label（形如 `sticker-<id>`）。
fn create_sticker_win(
    app: &tauri::AppHandle,
    id: i64,
    title: &str,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) -> tauri::Result<WebviewWindow> {
    let label = format!("sticker-{id}");
    WebviewWindowBuilder::new(app, &label, WebviewUrl::App("index.html".into()))
        .title(title)
        .inner_size(w as f64, h as f64)
        .position(x as f64, y as f64)
        .transparent(true)
        .decorations(false)
        .skip_taskbar(true)
        .resizable(true)
        .build()
}

/// 显示主控台窗口（不存在则创建）。
fn show_main(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

/// 托盘菜单动作分发。
fn dispatch_tray(app: &tauri::AppHandle, action: TrayAction) {
    match action {
        TrayAction::NewSticker => {
            let state = app.state::<AppState>().inner().clone();
            let new = NewSticker {
                title: "新建便签".into(),
                content: "# 标题\n\n在这里写内容...".into(),
                pos_x: 120,
                pos_y: 120,
                width: 400,
                height: 500,
                opacity: 0.9,
                ..Default::default()
            };
            match state.with_conn(|c| commands::create_sticker(c, &new)) {
                Ok(id) => {
                    let _ = create_sticker_win(app, id, &new.title, 120, 120, 400, 500);
                    events::emit_push_update(app, id);
                }
                Err(e) => tracing::warn!("托盘新建便签失败：{e:#}"),
            }
        }
        TrayAction::OpenMain => show_main(app),
        TrayAction::OpenSettings => {
            show_main(app);
            events::emit_to_label(app, "main", events::OPEN_SETTINGS, ());
        }
        TrayAction::Quit => {
            tracing::info!("托盘退出");
            app.exit(0);
        }
    }
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

            let handle = app.handle().clone();
            let state = app.state::<AppState>().inner().clone();

            // 系统托盘（新建便签/打开主控台/系统设置/退出）
            platform::tray::install(&handle, dispatch_tray)?;

            // 提醒调度器（10s 周期）
            reminder::scheduler::spawn(handle.clone(), state.clone());

            // 启动恢复：为数据库中已有便签重建窗口
            if let Ok(stickers) = state.with_conn(commands::list_stickers) {
                for s in stickers {
                    let _ = create_sticker_win(&handle, s.id, &s.title, s.pos_x, s.pos_y, s.width, s.height);
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            db_health,
            create_sticker_window,
            list_sticker_windows
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
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
