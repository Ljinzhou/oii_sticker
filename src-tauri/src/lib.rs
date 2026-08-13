// 数据层/repo 由阶段 3 的命令层接入前，先允许 dead_code（避免 40+ 噪音警告）；
// 阶段 3 完成命令接入后移除该属性。
#![allow(dead_code)]

use serde::Serialize;
use tauri::{Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

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
fn db_health(state: State<'_, AppState>) -> Result<DbHealth, String> {
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

/// 创建一个独立便签窗口（透明、无边框、不出现在任务栏），label = `sticker-<id>`。
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
            match state.with_conn(|c| create_sticker(c, &new)) {
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

// ═══════════════════ 业务命令层（前端 invoke 契约） ═══════════════════

#[tauri::command]
fn list_stickers_cmd(state: State<'_, AppState>) -> Result<Vec<models::Sticker>, String> {
    state
        .with_conn(commands::list_stickers)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_sticker_cmd(
    state: State<'_, AppState>,
    id: i64,
) -> Result<Option<models::Sticker>, String> {
    state
        .with_conn(|c| commands::get_sticker(c, id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn create_sticker_cmd(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    new: NewSticker,
) -> Result<i64, String> {
    let id = state
        .with_conn(|c| commands::create_sticker(c, &new))
        .map_err(|e| e.to_string())?;
    create_sticker_win(
        &app,
        id,
        &new.title,
        new.pos_x,
        new.pos_y,
        new.width,
        new.height,
    )
    .map_err(|e| e.to_string())?;
    events::emit_push_update(&app, id);
    Ok(id)
}

#[tauri::command]
fn update_sticker_cmd(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: i64,
    patch: db::sticker_repo::StickerPatch,
) -> Result<(), String> {
    state
        .with_conn(|c| commands::update_sticker(c, id, &patch))
        .map_err(|e| e.to_string())?;
    // 标题/尺寸变化同步到窗口
    if let Some(win) = app.get_webview_window(&format!("sticker-{id}")) {
        if let Some(title) = &patch.title {
            let _ = win.set_title(title);
        }
    }
    events::emit_push_update(&app, id);
    Ok(())
}

#[tauri::command]
fn delete_sticker_cmd(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> Result<(), String> {
    state
        .with_conn(|c| commands::delete_sticker(c, id))
        .map_err(|e| e.to_string())?;
    if let Some(win) = app.get_webview_window(&format!("sticker-{id}")) {
        let _ = win.close();
    }
    events::emit_push_update(&app, id);
    Ok(())
}

#[tauri::command]
fn set_reminder_cmd(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    attrs: models::StickerAttrs,
) -> Result<(), String> {
    state
        .with_conn(|c| commands::set_reminder(c, &attrs))
        .map_err(|e| e.to_string())?;
    events::emit_push_update(&app, attrs.sticker_id);
    Ok(())
}

#[tauri::command]
fn clear_reminder_cmd(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> Result<(), String> {
    state
        .with_conn(|c| commands::clear_reminder(c, id))
        .map_err(|e| e.to_string())?;
    events::emit_push_update(&app, id);
    Ok(())
}

#[tauri::command]
fn get_reminder_cmd(
    state: State<'_, AppState>,
    id: i64,
) -> Result<Option<models::StickerAttrs>, String> {
    state
        .with_conn(|c| commands::get_reminder(c, id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_config_cmd(state: State<'_, AppState>) -> Result<models::SystemConfig, String> {
    state
        .with_conn(commands::get_config)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_config_cmd(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    key: String,
    value: String,
) -> Result<(), String> {
    state
        .with_conn(|c| commands::set_config(c, &key, &value))
        .map_err(|e| e.to_string())?;
    let _ = state.refresh_config();
    events::emit_to_label(&app, "main", events::PREFS_UPDATED, ());
    Ok(())
}

#[tauri::command]
fn update_sticker_prefs_cmd(
    state: State<'_, AppState>,
    prefs: models::StickerPrefs,
) -> Result<(), String> {
    state
        .with_conn(|c| commands::update_sticker_prefs(c, &prefs))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn reset_sticker_prefs_cmd(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    state
        .with_conn(|c| commands::reset_sticker_prefs(c, id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn effective_prefs_cmd(
    state: State<'_, AppState>,
    id: i64,
) -> Result<models::EffectivePrefs, String> {
    let config = state.config().clone();
    state
        .with_conn(|c| commands::effective_prefs(c, &config, id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn toggle_todo_cmd(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: i64,
    line: usize,
) -> Result<bool, String> {
    let changed = state
        .with_conn(|c| commands::toggle_todo_in_sticker(c, id, line))
        .map_err(|e| e.to_string())?;
    if changed {
        events::emit_push_update(&app, id);
    }
    Ok(changed)
}

/// 斜杠命令查询结果（前端浮层展示 + 插入模板）。
#[derive(Serialize)]
struct SlashDto {
    id: String,
    name: String,
    category: String,
    hint: String,
    template: String,
}

#[tauri::command]
fn slash_query_cmd(query: String) -> Vec<SlashDto> {
    slash::matcher::filter(&slash::all_commands(), &query)
        .into_iter()
        .filter_map(|c| {
            let template = (c.insert)(&query)?;
            Some(SlashDto {
                id: c.id.to_string(),
                name: c.name.to_string(),
                category: c.category.to_string(),
                hint: c.hint.to_string(),
                template,
            })
        })
        .collect()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 初始化 tracing 日志（stdout）
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

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
                    let _ = create_sticker_win(
                        &handle, s.id, &s.title, s.pos_x, s.pos_y, s.width, s.height,
                    );
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            db_health,
            list_stickers_cmd,
            get_sticker_cmd,
            create_sticker_cmd,
            update_sticker_cmd,
            delete_sticker_cmd,
            set_reminder_cmd,
            clear_reminder_cmd,
            get_reminder_cmd,
            get_config_cmd,
            set_config_cmd,
            update_sticker_prefs_cmd,
            reset_sticker_prefs_cmd,
            effective_prefs_cmd,
            toggle_todo_cmd,
            slash_query_cmd
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
