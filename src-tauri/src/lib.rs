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

/// 创建一个独立便签窗口（透明、无边框、不出现在任务栏、不可最大化），label = `sticker-<id>`。
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
        .maximizable(false) // 禁用最大化（双击标题栏不触发）
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
    let result = state
        .with_conn(commands::list_stickers)
        .map_err(|e| e.to_string());
    tracing::debug!("[cmd] list_stickers_cmd → {:?}", result.as_ref().map(|v| v.len()));
    result
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
async fn create_sticker_cmd(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    new: NewSticker,
) -> Result<i64, String> {
    let state2 = state.inner().clone();
    let new_for_db = new.clone();
    let id = tauri::async_runtime::spawn_blocking(move || {
        state2.with_conn(|c| commands::create_sticker(c, &new_for_db))
    })
    .await
    .map_err(|e| format!("spawn_blocking 失败: {e}"))?
    .map_err(|e| e.to_string())?;

    // 窗口创建投递到主线程异步执行（避免在 IPC/async 线程同步建窗阻塞事件循环）
    let app2 = app.clone();
    let title = new.title.clone();
    let (x, y, w, h) = (new.pos_x, new.pos_y, new.width, new.height);
    app.run_on_main_thread(move || {
        let win = create_sticker_win(&app2, id, &title, x, y, w, h);
        tracing::info!(
            "[cmd] create_sticker_cmd id={id} title={title} pos=({x},{y}) win_ok={}",
            win.is_ok()
        );
    })
    .map_err(|e| format!("投递主线程失败: {e}"))?;

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
    // 标题/置顶/尺寸变化同步到窗口
    if let Some(win) = app.get_webview_window(&format!("sticker-{id}")) {
        if let Some(title) = &patch.title {
            let _ = win.set_title(title);
        }
        if let Some(on_top) = patch.always_on_top {
            let _ = win.set_always_on_top(on_top);
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
    // 锁顺序：先 conn 锁（读 prefs + sticker 背景色），再 config 读锁合并；
    // 与 refresh_config 的 conn→config 写锁顺序一致，避免死锁环。
    let (prefs, sticker_bg) = state
        .with_conn(|c| {
            let prefs = crate::db::prefs_repo::get(c, id)?.unwrap_or_default();
            let bg = crate::db::sticker_repo::get(c, id)?.and_then(|s| s.bg_color);
            Ok((prefs, bg))
        })
        .map_err(|e| e.to_string())?;
    let config = state.config().clone();
    Ok(config.effective(&prefs, sticker_bg.as_deref()))
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

/// 关闭主控台：按 system_config `main_close_behavior` 决定行为
/// （"hide" 隐藏到托盘 / "quit" 退出程序，默认 hide）。
#[tauri::command]
fn main_close_cmd(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let behavior = state
        .with_conn(|c| commands::get_config(c))
        .map(|cfg| cfg.get_or("main_close_behavior", "hide"))
        .unwrap_or_else(|e| {
            tracing::warn!("[cmd] main_close_cmd 读取配置失败：{e}");
            "hide".to_string()
        });
    tracing::info!("[cmd] main_close_cmd behavior={behavior}");
    if behavior == "quit" {
        app.exit(0);
    } else {
        if let Some(win) = app.get_webview_window("main") {
            let hidden = win.hide();
            tracing::info!("[cmd] main_close_cmd 隐藏主控台：{hidden:?}");
        } else {
            tracing::warn!("[cmd] main_close_cmd 未找到主控台窗口");
        }
    }
    Ok(())
}

/// 应用便签窗口状态：display 模式 → 禁止 resize（低透明收起）；
/// 其他模式 → 可 resize。
/// 注意：display 不做点击穿透（穿透会收不到"右键双击唤醒"，与设计手册冲突）。
#[tauri::command]
fn apply_window_state_cmd(
    app: tauri::AppHandle,
    id: i64,
    is_display: bool,
) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(&format!("sticker-{id}")) {
        win.set_resizable(!is_display)
            .map_err(|e| format!("设置 resize 失败: {e}"))?;
        // 确保不处于穿透状态，保证右键双击唤醒可用
        win.set_ignore_cursor_events(false)
            .map_err(|e| format!("取消穿透失败: {e}"))?;
        tracing::debug!("[cmd] apply_window_state id={id} is_display={is_display}");
    }
    Ok(())
}

/// 唤醒便签窗口：置前聚焦 + 可 resize（display 收起后使用）。
#[tauri::command]
fn wake_sticker_cmd(app: tauri::AppHandle, id: i64) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(&format!("sticker-{id}")) {
        win.set_ignore_cursor_events(false)
            .map_err(|e| format!("取消穿透失败: {e}"))?;
        win.set_resizable(true).map_err(|e| format!("设置 resize 失败: {e}"))?;
        let _ = win.show();
        let _ = win.set_focus();
        tracing::info!("[cmd] wake_sticker id={id}");
    }
    Ok(())
}

#[tauri::command]
fn debug_notify_cmd(app: tauri::AppHandle, title: String, body: String) -> Result<(), String> {
    crate::platform::notify::send(&app, &title, &body);
    Ok(())
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
    // 初始化 tracing 日志：默认 debug 级别（调试模式默认开启，输出详细
    // 操作/事件日志）；可用 RUST_LOG 环境变量覆盖。
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("debug")),
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

            // 启动恢复：为数据库中已有便签重建窗口；空库则创建默认展示便签
            let stickers = state
                .with_conn(commands::list_stickers)
                .unwrap_or_default();
            if stickers.is_empty() {
                // 首次运行：创建一条默认便签，便于查看效果
                let default = NewSticker {
                    title: "欢迎使用 oii_sticker".into(),
                    content: "# 欢迎使用 oii_sticker\n\n这是一张默认便签，可以：\n\n- 点击右上角 ✎ 进入编辑\n- 双击便签从收起状态唤醒\n- 点击 ⚙ 调整颜色与透明度\n\n## 任务清单\n\n- [ ] 试试勾选这个待办\n- [x] 已完成示例\n\n> 背景半透明、文字不透明。".into(),
                    pos_x: 200,
                    pos_y: 140,
                    width: 400,
                    height: 500,
                    opacity: 0.9,
                    bg_color: Some("#FFF4D6".into()),
                    ..Default::default()
                };
                if let Ok(id) = state.with_conn(|c| create_sticker(c, &default)) {
                    let _ = create_sticker_win(
                        &handle, id, &default.title, default.pos_x, default.pos_y,
                        default.width, default.height,
                    );
                }
            } else {
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
            debug_notify_cmd,
            main_close_cmd,
            apply_window_state_cmd,
            wake_sticker_cmd,
            slash_query_cmd
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
