// 数据层/repo 由阶段 3 的命令层接入前，先允许 dead_code（避免 40+ 噪音警告）；
// 阶段 3 完成命令接入后移除该属性。
#![allow(dead_code)]

use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_autostart::ManagerExt as AutostartManagerExt;

mod commands;
mod db;
mod editing;
mod events;
mod models;
mod platform;
mod slash;
mod state;
mod workspace;

use commands::create_sticker;
use db::sticker_repo::NewSticker;
use platform::tray::TrayAction;
use state::AppState;

/// 便签窗口创建参数（避免 create_sticker_win 参数过多）。
#[derive(Clone)]
struct StickerWinArgs {
    id: i64,
    title: String,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    always_on_top: bool,
}

impl StickerWinArgs {
    fn from_new(new: &NewSticker, id: i64) -> Self {
        Self {
            id,
            title: new.title.clone(),
            x: new.pos_x,
            y: new.pos_y,
            w: new.width,
            h: new.height,
            always_on_top: new.always_on_top,
        }
    }

    fn from_sticker(s: &models::Sticker) -> Self {
        Self {
            id: s.id,
            title: s.title.clone(),
            x: s.pos_x,
            y: s.pos_y,
            w: s.width,
            h: s.height,
            always_on_top: s.always_on_top,
        }
    }
}

/// 便签窗口最小尺寸（逻辑像素）。交互/编辑模式下禁止缩到比这更小，
/// 避免用户不小心把窗口缩得无法再用边缘拖拽调整大小。
const STICKER_MIN_W: f64 = 240.0;
const STICKER_MIN_H: f64 = 170.0;

/// 便签“重置窗口大小与位置”的默认尺寸（与新建便签默认一致）。
const STICKER_DEFAULT_W: f64 = 400.0;
const STICKER_DEFAULT_H: f64 = 500.0;

/// 开机自启时附加的命令行参数（Windows 注册表 / macOS LaunchAgent / Linux XDG 均写入）。
/// 程序据此识别“由登录自启拉起”，从而不弹出主控台窗口（托盘仍在，可随时呼出）。
pub const AUTOSTART_ARG: &str = "--autostart";

/// 创建一个独立便签窗口（透明、无边框、不出现在任务栏、不可最大化），label = `sticker-<id>`。
/// 创建后立即应用置顶（Builder 不提供置顶选项，须显式 set_always_on_top）。
fn create_sticker_win(app: &tauri::AppHandle, args: StickerWinArgs) -> tauri::Result<WebviewWindow> {
    let label = format!("sticker-{}", args.id);
    let win = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("index.html".into()))
        .title(&args.title)
        .inner_size(args.w as f64, args.h as f64)
        .min_inner_size(STICKER_MIN_W, STICKER_MIN_H)
        .position(args.x as f64, args.y as f64)
        .transparent(true)
        .decorations(false)
        .skip_taskbar(true)
        .maximizable(false) // 禁用最大化（双击标题栏不触发）
        .resizable(true)
        .build()?;
    let _ = win.set_always_on_top(args.always_on_top);
    Ok(win)
}

/// 创建独立 Todo 窗口。OS 关闭请求隐藏窗口，数据持续由 SQLite 保存。
fn create_todo_win(
    app: &tauri::AppHandle,
    id: &str,
    always_on_top: bool,
) -> tauri::Result<WebviewWindow> {
    let label = format!("todo-{id}");
    let win = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("index.html".into()))
        .title("任务")
        .inner_size(440.0, 620.0)
        .min_inner_size(320.0, 420.0)
        .decorations(false)
        .skip_taskbar(true)
        .maximizable(false)
        .resizable(true)
        .build()?;
    if let Err(e) = win.set_always_on_top(always_on_top) {
        tracing::warn!("设置 Todo 窗口置顶失败 label={label} always_on_top={always_on_top}: {e}");
    }
    Ok(win)
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
            match state.with_conn_path(|c, db| create_sticker(c, &new, db)) {
                Ok(id) => {
                    // 新建便签按系统默认置顶配置应用（default_sticker_always_on_top）
                    let on_top = state
                        .with_conn(commands::get_config)
                        .map(|cfg| cfg.get_or("default_sticker_always_on_top", "1") == "1")
                        .unwrap_or(true);
                    let args = StickerWinArgs {
                        id,
                        title: new.title.clone(),
                        x: 120,
                        y: 120,
                        w: 400,
                        h: 500,
                        always_on_top: on_top,
                    };
                    let _ = create_sticker_win(app, args);
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
        .with_conn_path(|c, db| commands::get_sticker(c, id, db))
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
        state2.with_conn_path(|c, db| commands::create_sticker(c, &new_for_db, db))
    })
    .await
    .map_err(|e| format!("spawn_blocking 失败: {e}"))?
    .map_err(|e| e.to_string())?;

    // Windows WebView2 要求建窗发生在 async 命令的工作线程，不能进入主线程任务。
    let args = StickerWinArgs::from_new(&new, id);
    let title = args.title.clone();
    let win = create_sticker_win(&app, args)
        .map_err(|e| format!("创建便签窗口失败: {e}"))?;
    tracing::info!("[cmd] create_sticker_cmd id={id} title={title} win_ok=true");
    let _ = win.set_focus();

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
        .with_conn_path(|c, db| commands::update_sticker(c, id, &patch, db))
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
        .with_conn_path(|c, db| commands::delete_sticker(c, id, db))
        .map_err(|e| e.to_string())?;
    if let Some(win) = app.get_webview_window(&format!("sticker-{id}")) {
        let _ = win.close();
    }
    events::emit_push_update(&app, id);
    Ok(())
}

// ── 便签分组 ──

#[tauri::command]
fn group_list_cmd(state: State<'_, AppState>) -> Result<Vec<models::StickerGroup>, String> {
    state.with_conn(commands::list_groups).map_err(|e| e.to_string())
}

#[tauri::command]
fn group_create_cmd(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<models::StickerGroup, String> {
    let g = state
        .with_conn(|c| commands::create_group(c, &name))
        .map_err(|e| e.to_string())?;
    events::emit_push_update(&app, 0);
    Ok(g)
}

#[tauri::command]
fn group_rename_cmd(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: i64,
    name: String,
) -> Result<(), String> {
    state
        .with_conn(|c| commands::rename_group(c, id, &name))
        .map_err(|e| e.to_string())?;
    events::emit_push_update(&app, 0);
    Ok(())
}

#[tauri::command]
fn group_delete_cmd(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: i64,
    mode: String,
) -> Result<usize, String> {
    let removed = state
        .with_conn_path(|c, db| commands::delete_group(c, id, &mode, db))
        .map_err(|e| e.to_string())?;
    events::emit_push_update(&app, 0);
    Ok(removed)
}

/// 移动便签到分组；group_id=None 表示回默认组。
#[tauri::command]
fn move_sticker_group_cmd(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    sticker_id: i64,
    group_id: Option<i64>,
) -> Result<(), String> {
    state
        .with_conn(|c| commands::move_sticker_group(c, sticker_id, group_id))
        .map_err(|e| e.to_string())?;
    events::emit_push_update(&app, 0);
    Ok(())
}

/// 抓取网页 <title>，供及时预览「粘贴链接自动转 [title](url)」使用。
/// 非 http(s) 链接 / 请求失败 / 无 <title> 时返回 None（前端保持 `[](url)` 占位）。
#[tauri::command]
async fn fetch_page_title_cmd(url: String) -> Result<Option<String>, String> {
    let trimmed = url.trim();
    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        return Ok(None);
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) oii_sticker/0.1")
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| format!("HTTP 客户端初始化失败：{e}"))?;
    let resp = client
        .get(trimmed)
        .send()
        .await
        .map_err(|e| format!("请求 {trimmed} 失败：{e}"))?;
    if !resp.status().is_success() {
        return Ok(None);
    }
    let bytes = resp.bytes().await.map_err(|e| format!("读取响应失败：{e}"))?;
    let head_size = bytes.len().min(512 * 1024);
    let html = String::from_utf8_lossy(&bytes[..head_size]);
    Ok(extract_page_title(&html))
}

/// 从 HTML 中提取 `<title>…</title>` 文本（去标签/HTML 实体/空白，超长截断 120 字符）。
fn extract_page_title(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let tag = lower.find("<title")?;
    let open = html[tag..].find('>')? + tag + 1;
    let close = lower[open..].find("</title")? + open;
    if close <= open {
        return None;
    }
    let raw = &html[open..close];

    // 去除内部标签
    let mut text = String::with_capacity(raw.len());
    let mut in_tag = false;
    for ch in raw.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => text.push(ch),
            _ => {}
        }
    }

    // 还原常见 HTML 实体
    let named = [
        ("&amp;", "&"),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&quot;", "\""),
        ("&#39;", "'"),
        ("&apos;", "'"),
        ("&nbsp;", " "),
    ];
    for (from, to) in named {
        text = text.replace(from, to);
    }
    // 数字实体（十进制/十六进制字符码）
    let chars: Vec<char> = text.chars().collect();
    let mut decoded = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '&' && i + 2 < chars.len() && chars[i + 1] == '#' {
            let mut j = i + 2;
            let mut radix = 10;
            if chars[j] == 'x' || chars[j] == 'X' {
                radix = 16;
                j += 1;
            }
            let start = j;
            while j < chars.len() && chars[j].is_ascii_hexdigit() {
                j += 1;
            }
            if j < chars.len() && chars[j] == ';' && j > start {
                let code_str: String = chars[start..j].iter().collect();
                if let Ok(code) = u32::from_str_radix(&code_str, radix) {
                    if let Some(c) = char::from_u32(code) {
                        decoded.push(c);
                        i = j + 1;
                        continue;
                    }
                }
            }
        }
        decoded.push(chars[i]);
        i += 1;
    }

    let joined: String = decoded.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = joined.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut title = trimmed.to_string();
    if title.chars().count() > 120 {
        title = title.chars().take(120).collect::<String>() + "…";
    }
    Some(title)
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
    if key == "default_todo_always_on_top" {
        let always_on_top = value == "1";
        for (label, win) in app.webview_windows().iter() {
            if label.starts_with("todo-") {
                if let Err(e) = win.set_always_on_top(always_on_top) {
                    tracing::warn!("更新 Todo 窗口置顶失败 label={label} always_on_top={always_on_top}: {e}");
                }
            }
        }
    }
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
        .with_conn_path(|c, db| commands::toggle_todo_in_sticker(c, id, line, db))
        .map_err(|e| e.to_string())?;
    if changed {
        events::emit_push_update(&app, id);
    }
    Ok(changed)
}

#[tauri::command]
fn get_todo_block_cmd(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<models::TodoBlock>, String> {
    state.with_conn(|c| commands::get_todo_block(c, &id)).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_todo_for_sticker_cmd(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    sticker_id: i64,
) -> Result<Vec<models::TodoBlock>, String> {
    let (blocks, retagged) = state
        .with_conn(|c| commands::list_todo_blocks(c, sticker_id))
        .map_err(|e| e.to_string())?;
    if retagged {
        // 补写了孤儿块标记：通知便签窗口刷新正文，使任务卡即时展示
        events::emit_push_update(&app, sticker_id);
    }
    Ok(blocks)
}

#[tauri::command]
fn create_todo_block_cmd(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    sticker_id: i64,
    parent_id: Option<String>,
) -> Result<models::TodoBlock, String> {
    let block = state
        .with_conn(|c| commands::create_todo_block(c, sticker_id, parent_id.as_deref()))
        .map_err(|e| e.to_string())?;
    events::emit_todo_updated(&app, block.sticker_id, &block.id);
    Ok(block)
}

#[tauri::command]
fn update_todo_block_cmd(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
    patch: models::TodoPatch,
) -> Result<models::TodoBlock, String> {
    let block = state.with_conn(|c| commands::update_todo_block(c, &id, &patch)).map_err(|e| e.to_string())?;
    events::emit_todo_updated(&app, block.sticker_id, &block.id);
    Ok(block)
}

#[tauri::command]
fn delete_todo_block_cmd(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    if let Some(sticker_id) = state.with_conn(|c| commands::delete_todo_block(c, &id)).map_err(|e| e.to_string())? {
        events::emit_todo_updated(&app, sticker_id, &id);
        // 删除根任务会同步移除便签正文中的标记行，需刷新便签窗口内容
        events::emit_push_update(&app, sticker_id);
    }
    Ok(())
}

#[tauri::command]
fn sync_todo_marker_cmd(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    sticker_id: i64,
    id: String,
) -> Result<(), String> {
    let changed = state
        .with_conn_path(|c, db| commands::sync_todo_marker(c, sticker_id, &id, db))
        .map_err(|e| e.to_string())?;
    if changed {
        events::emit_push_update(&app, sticker_id);
    }
    Ok(())
}

/// Todo 窗口在场状态通报：编辑 Todo 期间所属便签不自动收起回展示模式。
#[tauri::command]
fn notify_todo_presence_cmd(app: tauri::AppHandle, sticker_id: i64, present: bool) -> Result<(), String> {
    events::emit_to_label(&app, &format!("sticker-{sticker_id}"), "sticky://todo-presence", present);
    Ok(())
}

/// 任务拖拽排序：ids 为该分组（根或同父子任务）的完整新顺序。
#[tauri::command]
fn reorder_todo_cmd(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> Result<(), String> {
    if let Some(sticker_id) = state
        .with_conn(|c| commands::reorder_todo(c, &ids))
        .map_err(|e| e.to_string())?
    {
        events::emit_todo_updated(&app, sticker_id, &ids[0]);
    }
    Ok(())
}

#[tauri::command]
async fn open_todo_window_cmd(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let state = state.inner().clone();
    let id_for_db = id.clone();
    let todo = state
        .with_conn_async(move |c| commands::get_todo_block(c, &id_for_db))
        .await
        .map_err(|e| e.to_string())?;
    if todo.is_none() {
        return Err("Todo 块不存在".into());
    }
    let always_on_top = state
        .with_conn(commands::get_config)
        .map(|cfg| cfg.get_or("default_todo_always_on_top", "1") == "1")
        .unwrap_or(true);
    let label = format!("todo-{id}");
    if let Some(win) = app.get_webview_window(&label) {
        if let Err(e) = win.set_always_on_top(always_on_top) {
            tracing::warn!("更新 Todo 窗口置顶失败 label={label} always_on_top={always_on_top}: {e}");
        }
        let _ = win.show();
        let _ = win.unminimize();
        return win.set_focus().map_err(|e| format!("聚焦 Todo 窗口失败: {e}"));
    }
    let win = create_todo_win(&app, &id, always_on_top)
        .map_err(|e| format!("创建 Todo 窗口失败: {e}"))?;
    win.set_focus()
        .map_err(|e| format!("聚焦 Todo 窗口失败: {e}"))?;
    tracing::info!("[cmd] open_todo_window id={id} 创建成功");
    Ok(())
}

#[tauri::command]
fn close_todo_window_cmd(app: tauri::AppHandle, id: String) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(&format!("todo-{id}")) {
        win.hide().map_err(|e| format!("隐藏 Todo 窗口失败: {e}"))?;
    }
    Ok(())
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


/// 开机自启在设置页需要的平台信息：状态 + 平台机制 + 启动参数 + 可执行文件。
#[derive(Serialize)]
pub struct AutostartInfo {
    /// 当前是否启用了开机自启
    pub enabled: bool,
    /// 运行平台：windows / macos / linux / unsupported
    pub platform: &'static str,
    /// 各平台采用的自启动机制描述
    pub mechanism: &'static str,
    /// 自启动时附加的命令行参数（与插件注册保持一致）
    pub launch_args: Vec<String>,
    /// 将由系统于登录时拉起（可执行）的路径
    pub executable: String,
    /// 登录自启启动时主控台是否自动隐藏（--autostart 生效时为 true）
    pub hides_main_on_autostart: bool,
}

/// 当前运行平台标识。
pub fn autostart_platform() -> &'static str {
    #[cfg(target_os = "windows")]
    { "windows" }
    #[cfg(target_os = "macos")]
    { "macos" }
    #[cfg(target_os = "linux")]
    { "linux" }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    { "unsupported" }
}

/// 各平台的自启动机制说明（与 tauri-plugin-autostart 的实现一一对应）。
pub fn autostart_mechanism() -> &'static str {
    #[cfg(target_os = "windows")]
    { "注册表 Run 键（HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run）" }
    #[cfg(target_os = "macos")]
    { "macOS LaunchAgent（~/Library/LaunchAgents）" }
    #[cfg(target_os = "linux")]
    { "XDG 桌面自动启动（~/.config/autostart；AppImage 使用镜像路径）" }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    { "当前平台暂不支持系统级自启动" }
}

/// 汇总当前自启动状态（插件状态 + 平台信息）。
fn current_autostart_info(app: &tauri::AppHandle) -> AutostartInfo {
    let enabled = AutostartManagerExt::autolaunch(app)
        .is_enabled()
        .unwrap_or(false);
    AutostartInfo {
        enabled,
        platform: autostart_platform(),
        mechanism: autostart_mechanism(),
        launch_args: vec![AUTOSTART_ARG.to_string()],
        executable: std::env::current_exe()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| String::new()),
        hides_main_on_autostart: true,
    }
}

/// 读取开机自启状态（供设置页展示当前平台机制与开关状态）。
#[tauri::command]
fn autostart_get_cmd(app: tauri::AppHandle) -> Result<AutostartInfo, String> {
    Ok(current_autostart_info(&app))
}

/// 开启/关闭开机自启；始终返回操作后的最新状态（含平台机制信息）。
#[tauri::command]
fn autostart_set_cmd(app: tauri::AppHandle, enabled: bool) -> Result<AutostartInfo, String> {
    let launch = AutostartManagerExt::autolaunch(&app);
    if enabled {
        launch
            .enable()
            .map_err(|e| format!("开启开机自启失败：{e}"))?;
        tracing::info!("[autostart] 已开启开机自启（{}）", autostart_mechanism());
    } else {
        launch
            .disable()
            .map_err(|e| format!("关闭开机自启失败：{e}"))?;
        tracing::info!("[autostart] 已关闭开机自启");
    }
    Ok(current_autostart_info(&app))
}

/// 关闭主控台：按 system_config `main_close_behavior` 决定行为
/// （"hide" 隐藏到托盘 / "quit" 退出程序，默认 hide）。
#[tauri::command]
fn main_close_cmd(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let behavior = state
        .with_conn(commands::get_config)
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

/// 应用便签窗口状态：display 模式 → 点击穿透 + 禁止 resize（锁尺寸）；
/// 其他模式 → 正常交互 + 可 resize。
/// 穿透状态下由全局鼠标钩子（platform::mouse_hook）检测右键双击唤醒。
#[tauri::command]
fn apply_window_state_cmd(
    app: tauri::AppHandle,
    id: i64,
    is_display: bool,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    state.set_display_window(id, is_display);
    if let Some(win) = app.get_webview_window(&format!("sticker-{id}")) {
        if is_display {
            let size = win.outer_size().map_err(|e| format!("读取尺寸失败: {e}"))?;
            win.set_min_size(Some(size))
                .map_err(|e| format!("设置最小尺寸失败: {e}"))?;
            win.set_max_size(Some(size))
                .map_err(|e| format!("设置最大尺寸失败: {e}"))?;
            win.set_resizable(false)
                .map_err(|e| format!("设置 resize 失败: {e}"))?;
            // 全穿透：窗口不拦截任何鼠标事件（含左键选中文字），右键双击由钩子唤醒
            win.set_ignore_cursor_events(true)
                .map_err(|e| format!("设置点击穿透失败: {e}"))?;
        } else {
            // 交互/编辑模式：解除 display 锁定的尺寸，但仍保留最小窗口尺寸，
            // 防止用户把窗口缩到无法再调整。
            win.set_min_size(Some(tauri::Size::Logical(tauri::LogicalSize::new(
                STICKER_MIN_W, STICKER_MIN_H,
            ))))
            .map_err(|e| format!("设置最小尺寸失败: {e}"))?;
            win.set_max_size(None::<tauri::Size>)
                .map_err(|e| format!("清除最大尺寸失败: {e}"))?;
            win.set_resizable(true)
                .map_err(|e| format!("设置 resize 失败: {e}"))?;
            win.set_ignore_cursor_events(false)
                .map_err(|e| format!("取消穿透失败: {e}"))?;
        }
        tracing::debug!("[cmd] apply_window_state id={id} is_display={is_display}");
    }
    Ok(())
}

/// 重置便签窗口大小与位置：恢复默认 400×500 并居中到当前显示器；
/// 窗口未打开时写入默认几何（下次显示/重启恢复时生效）；
/// 展示模式保持尺寸锁定与点击穿透。
#[tauri::command]
fn reset_sticker_window_cmd(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: i64,
    is_display: bool,
) -> Result<(), String> {
    state.set_display_window(id, is_display);
    let label = format!("sticker-{id}");
    if let Some(win) = app.get_webview_window(&label) {
        // 1) 先解除展示模式的尺寸锁，再恢复默认尺寸
        let _ = win.set_min_size(None::<tauri::Size>);
        let _ = win.set_max_size(None::<tauri::Size>);
        win.set_size(tauri::Size::Logical(tauri::LogicalSize::new(
            STICKER_DEFAULT_W, STICKER_DEFAULT_H,
        )))
        .map_err(|e| format!("重置窗口尺寸失败: {e}"))?;
        // 2) 居中到当前显示器（物理像素→逻辑像素换算）
        if let Some(mon) = win
            .current_monitor()
            .map_err(|e| format!("读取显示器失败: {e}"))?
        {
            let scale = mon.scale_factor();
            let mw = mon.size().width as f64 / scale;
            let mh = mon.size().height as f64 / scale;
            let x = ((mw - STICKER_DEFAULT_W) / 2.0).max(0.0);
            let y = ((mh - STICKER_DEFAULT_H) / 2.0).max(0.0);
            let _ = win.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(x, y)));
        }
        // 3) 恢复窗口态：展示模式重新锁尺寸并全穿透；交互模式保持可调整
        if is_display {
            let size = win.outer_size().map_err(|e| format!("读取尺寸失败: {e}"))?;
            win.set_min_size(Some(size))
                .map_err(|e| format!("设置最小尺寸失败: {e}"))?;
            win.set_max_size(Some(size))
                .map_err(|e| format!("设置最大尺寸失败: {e}"))?;
            win.set_resizable(false)
                .map_err(|e| format!("设置 resize 失败: {e}"))?;
            win.set_ignore_cursor_events(true)
                .map_err(|e| format!("设置点击穿透失败: {e}"))?;
        } else {
            win.set_min_size(Some(tauri::Size::Logical(tauri::LogicalSize::new(
                STICKER_MIN_W, STICKER_MIN_H,
            ))))
            .map_err(|e| format!("设置最小尺寸失败: {e}"))?;
            win.set_max_size(None::<tauri::Size>)
                .map_err(|e| format!("清除最大尺寸失败: {e}"))?;
            win.set_resizable(true)
                .map_err(|e| format!("设置 resize 失败: {e}"))?;
            win.set_ignore_cursor_events(false)
                .map_err(|e| format!("取消穿透失败: {e}"))?;
        }
        // 4) 落库：新几何 + 显示态（重启/崩溃恢复按此还原）
        let pos = win.outer_position().map_err(|e| format!("读取位置失败: {e}"))?;
        let size = win.outer_size().map_err(|e| format!("读取尺寸失败: {e}"))?;
        state
            .with_conn(|c| {
                db::sticker_repo::update_window_bounds(
                    c, id, pos.x, pos.y, size.width as i32, size.height as i32,
                )
            })
            .map_err(|e| format!("保存窗口几何失败: {e}"))?;
        tracing::info!("[cmd] reset_sticker_window id={id} 已重置为 400x500 并居中");
    } else {
        // 窗口未打开：写入默认几何，供下次显示/重启恢复使用
        state
            .with_conn(|c| {
                db::sticker_repo::update_window_bounds(
                    c,
                    id,
                    200,
                    150,
                    STICKER_DEFAULT_W as i32,
                    STICKER_DEFAULT_H as i32,
                )
            })
            .map_err(|e| format!("保存默认几何失败: {e}"))?;
        tracing::info!("[cmd] reset_sticker_window id={id} 窗口未打开，已写入默认几何");
    }
    Ok(())
}

/// 唤醒便签窗口：置前聚焦 + 可 resize + 恢复置顶（display 收起后使用）。
/// 防御：窗口不存在（曾被销毁）时按数据库记录经主线程重建，保证主控台
/// "显示"按钮始终有效；同步清除 display 模式锁定的 min/max 尺寸
/// （与鼠标钩子唤醒路径一致，否则 set_resizable(true) 不生效）。
#[tauri::command]
async fn wake_sticker_cmd(app: tauri::AppHandle, id: i64) -> Result<(), String> {
    let label = format!("sticker-{id}");
    // 先取数据库记录（含 always_on_top），窗口存在与否都要恢复置顶
    let state = app.state::<AppState>().inner().clone();
    let s = state
        .with_conn_path_async(move |c, db| commands::get_sticker(c, id, db))
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("便签 #{id} 不存在，无法唤醒"))?;
    let win = if let Some(win) = app.get_webview_window(&label) {
        win
    } else {
        let rebuilt = create_sticker_win(&app, StickerWinArgs::from_sticker(&s))
            .map_err(|e| format!("重建便签窗口失败: {e}"))?;
        tracing::info!("[cmd] wake_sticker id={id} 窗口不存在，已按数据库记录重建");
        rebuilt
    };
    win.set_always_on_top(s.always_on_top)
        .map_err(|e| format!("恢复置顶失败: {e}"))?;
    win.set_ignore_cursor_events(false)
        .map_err(|e| format!("取消穿透失败: {e}"))?;
    win.set_resizable(true).map_err(|e| format!("设置 resize 失败: {e}"))?;
    win.set_min_size(Some(tauri::Size::Logical(tauri::LogicalSize::new(
        STICKER_MIN_W, STICKER_MIN_H,
    ))))
    .map_err(|e| format!("设置最小尺寸失败: {e}"))?;
    win.set_max_size(None::<tauri::Size>)
        .map_err(|e| format!("清除最大尺寸失败: {e}"))?;
    let _ = win.show();
    let _ = win.set_focus();
    // 标记为显示态（启动/崩溃恢复时按此决定是否重建窗口）
    let _ = state.with_conn(|c| db::sticker_repo::update_window_hidden(c, id, false));
    // 前端隐藏时已释放内容引用（releaseData）；重开必须推送刷新事件，
    // 让 StickerWindow 的 sticky://push-update 监听重新 load() 恢复数据。
    events::emit_push_update(&app, id);
    tracing::info!("[cmd] wake_sticker id={id} always_on_top={}", s.always_on_top);
    Ok(())
}

/// 列出当前**可见**的便签窗口 id（隐藏窗口不计入"打开"）。
#[tauri::command]
fn list_open_sticker_ids_cmd(app: tauri::AppHandle) -> Vec<i64> {
    app.webview_windows()
        .iter()
        .filter_map(|(label, win)| {
            let id = label
                .strip_prefix("sticker-")
                .and_then(|s| s.parse::<i64>().ok())?;
            win.is_visible().ok().filter(|v| *v)?;
            Some(id)
        })
        .collect()
}

/// 隐藏便签窗口（数据保留，主控台显示"显示"按钮）。
#[tauri::command]
fn hide_sticker_cmd(app: tauri::AppHandle, id: i64) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(&format!("sticker-{id}")) {
        win.hide().map_err(|e| format!("隐藏窗口失败: {e}"))?;
        tracing::info!("[cmd] hide_sticker id={id}");
        // 持久化隐藏状态：即使程序被异常终止，下次启动也不会再自动显示
        if let Some(state) = app.try_state::<AppState>() {
            let _ = state.with_conn(|c| db::sticker_repo::update_window_hidden(c, id, true));
        }
        // 广播 push-update：主控台收到后刷新 openIds，把按钮切到"显示"
        events::emit_push_update(&app, id);
    } else {
        tracing::warn!("[cmd] hide_sticker id={id} 窗口不存在");
    }
    Ok(())
}

/// 程序退出前保存各便签窗口的显示/隐藏状态与位置/尺寸：
/// - 当前可见的窗口 → 记录最新几何并标记为显示；
/// - 隐藏/无窗口的便签 → 标记为隐藏（保留上次几何，重新显示时恢复）。
/// 供启动时只恢复上次"显示"的便签，并在关闭时的位置/尺寸展示。
fn persist_sticker_window_states(app: &tauri::AppHandle) {
    use std::collections::HashMap;
    // label → (visible, x, y, w, h)；不可见窗口不记录几何（沿用库内旧值）。
    let mut live: HashMap<i64, (bool, i32, i32, i32, i32)> = HashMap::new();
    for (label, win) in app.webview_windows() {
        let Some(id) = label
            .strip_prefix("sticker-")
            .and_then(|s| s.parse::<i64>().ok())
        else {
            continue;
        };
        let visible = win.is_visible().unwrap_or(false);
        if visible {
            let x = win.outer_position().map(|p| p.x).unwrap_or(0);
            let y = win.outer_position().map(|p| p.y).unwrap_or(0);
            let w = win.outer_size().map(|s| s.width as i32).unwrap_or(0);
            let h = win.outer_size().map(|s| s.height as i32).unwrap_or(0);
            live.insert(id, (true, x, y, w, h));
        } else {
            live.insert(id, (false, 0, 0, 0, 0));
        }
    }

    let state = app.state::<AppState>().inner().clone();
    let stickers = state.with_conn(db::sticker_repo::list_all).unwrap_or_default();
    let mut saved = 0usize;
    for s in stickers {
        let ok = match live.get(&s.id) {
            Some(&(true, x, y, w, h)) => state
                .with_conn(|c| db::sticker_repo::update_window_bounds(c, s.id, x, y, w, h))
                .is_ok(),
            _ => state
                .with_conn(|c| db::sticker_repo::update_window_hidden(c, s.id, true))
                .is_ok(),
        };
        if ok {
            saved += 1;
        }
    }
    tracing::info!("[exit] 已保存 {saved} 个便签窗口状态（显示/隐藏 + 几何）");
}

/// 检查更新：查询 GitHub 最新 release 版本号并对比当前版本。
/// 返回最新版本号、是否有更新、当前版本号。
#[derive(Serialize)]
struct UpdateInfo {
    latest: Option<String>,
    current: String,
    has_update: bool,
    error: Option<String>,
}

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const GITHUB_REPO: &str = "Ljinzhou/oii_sticker";

#[tauri::command]
async fn check_update_cmd() -> UpdateInfo {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("oii_sticker/check-update")
        .build()
        .map_err(|e| format!("HTTP 客户端初始化失败：{e}"));
    let client = match client {
        Ok(c) => c,
        Err(e) => {
            return UpdateInfo { latest: None, current: APP_VERSION.to_string(), has_update: false, error: Some(e) }
        }
    };
    let url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");
    match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => {
            match resp.text().await {
                Ok(body) => {
                    // 手动解析 JSON 中 "tag_name":"..." 字段，避免额外引入 serde_json。
                    let latest = extract_json_tag_name(&body);
                    let has_update = latest.as_deref().map(|l| semantic_newer(l, APP_VERSION)).unwrap_or(false);
                    UpdateInfo { latest, current: APP_VERSION.to_string(), has_update, error: None }
                }
                Err(e) => UpdateInfo { latest: None, current: APP_VERSION.to_string(), has_update: false, error: Some(format!("读取更新信息失败：{e}")) },
            }
        }
        Ok(_) => UpdateInfo { latest: None, current: APP_VERSION.to_string(), has_update: false, error: Some("仓库无发布版本或检查失败".to_string()) },
        Err(e) => UpdateInfo { latest: None, current: APP_VERSION.to_string(), has_update: false, error: Some(format!("网络请求失败：{e}")) },
    }
}

/// 从 GitHub release JSON 中提取 `"tag_name": "..."` 的值（简单字符串解析）。
fn extract_json_tag_name(body: &str) -> Option<String> {
    let key = "\"tag_name\"";
    let idx = body.find(key)?;
    let after = &body[idx + key.len()..];
    let colon = after.find(':')?;
    let value = after[colon + 1..].trim_start();
    let value = value.trim_start_matches('"');
    let end = value.find('"')?;
    let raw = &value[..end];
    if raw.is_empty() {
        None
    } else {
        Some(raw.trim_start_matches('v').to_string())
    }
}

/// 比较两个点分语义版本，`newer` 是否晚于 `cur`（仅比较前三位数字）。
fn semantic_newer(newer: &str, cur: &str) -> bool {
    let parse = |v: &str| -> Vec<u32> { v.split('.').filter_map(|s| s.parse::<u32>().ok()).collect() };
    let a = parse(newer);
    let b = parse(cur);
    for (x, y) in a.iter().zip(b.iter()) {
        if x != y {
            return x > y;
        }
    }
    a.len() > b.len()
}

/// 用系统默认浏览器打开外部链接（绝不在内嵌 WebView 中导航，避免覆盖便签内容）。
/// 仅允许 http(s) 协议。
#[tauri::command]
fn open_external_cmd(app: tauri::AppHandle, url: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let lower = url.trim().to_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err(format!("仅允许打开 http(s) 链接：{url}"));
    }
    app.opener()
        .open_url(url.trim(), None::<&str>)
        .map_err(|e| format!("打开链接失败: {e}"))
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

// ═══════════════════ 工作空间命令（多工作空间） ═══════════════════

fn workspace_to_dto(w: workspace::layout::WorkspaceEntry) -> models::WorkspaceEntryDto {
    models::WorkspaceEntryDto {
        id: w.id,
        name: w.name,
        path: w.path,
        created_at: w.created_at,
    }
}

/// 工作空间 DB 路径：<root>/data/index.db。
fn entry_path_db(root: &Path) -> PathBuf {
    root.join("data").join("index.db")
}

#[tauri::command]
fn workspace_list_cmd(state: State<'_, AppState>) -> Result<Vec<models::WorkspaceEntryDto>, String> {
    workspace::cmds::list(&state.registry_path())
        .map(|v| v.into_iter().map(workspace_to_dto).collect())
        .map_err(|e| e.to_string())
}

/// 当前激活的工作空间（注册表无 current 时返回 None）。
#[tauri::command]
fn workspace_current_cmd(
    state: State<'_, AppState>,
) -> Result<Option<models::WorkspaceEntryDto>, String> {
    workspace::cmds::current(&state.registry_path())
        .map(|v| v.map(workspace_to_dto))
        .map_err(|e| e.to_string())
}

/// 新建工作空间：建目录 + 注册表；无当前项时自动激活（非首次引导，不迁移）。
/// 目标目录必须不存在或为空；非空返回 `DEST_NOT_EMPTY:` 前缀错误，由前端
/// 弹确认后改用其下的 oiistiker_workspace 子目录重试。
#[tauri::command]
fn workspace_create_cmd(
    state: State<'_, AppState>,
    path: String,
    name: Option<String>,
) -> Result<models::WorkspaceEntryDto, String> {
    workspace::layout::ensure_empty_dest(Path::new(&path)).map_err(|e| e.to_string())?;
    let entry = workspace::cmds::create(&state.registry_path(), Path::new(&path), name.as_deref())
        .map_err(|e| e.to_string())?;
    Ok(workspace_to_dto(entry))
}

/// 切换当前工作空间：注册表更新 + 关闭全部便签窗口 + DB 重连（switch_db）+ 主控台刷新。
#[tauri::command]
fn workspace_switch_cmd(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let entry = workspace::cmds::switch(&state.registry_path(), &id).map_err(|e| e.to_string())?;
    // 切换前必须关闭所有 open 的便签窗口（含隐藏的）：粘旧工作空间数据的
    // 窗口若继续存在，编辑会静默 no-op 或写错 md。UI 亦承诺"所有便签窗口将被关闭"。
    for (label, win) in app.webview_windows().iter() {
        if label.starts_with("sticker-") {
            let _ = win.close();
            tracing::info!("[switch] 关闭便签窗口 {label}");
        }
    }
    let db = PathBuf::from(&entry.path).join("data").join("index.db");
    state.switch_db(&db).map_err(|e| e.to_string())?;
    events::emit_push_update(&app, 0);
    Ok(())
}

#[tauri::command]
fn workspace_destroy_cmd(state: State<'_, AppState>, id: String) -> Result<(), String> {
    workspace::cmds::destroy(&state.registry_path(), &id).map_err(|e| e.to_string())
}

/// 首次引导默认路径：用户文档目录下默认工作空间（layout::default_root）。
#[tauri::command]
fn workspace_default_path_cmd() -> Result<String, String> {
    workspace::layout::default_root()
        .map(|p| p.to_string_lossy().into_owned())
        .ok_or_else(|| "无法定位用户文档目录，请手动输入工作空间路径".to_string())
}

/// 备份：在线快照 + zip 打包（排除 cache/）。
/// 仅允许备份当前激活的工作空间（离线快照只对当前 DB 语义成立）。
/// 返回 zip 文件大小（字节）；不推送事件（保持静默）。
#[tauri::command]
fn workspace_backup_cmd(
    state: State<'_, AppState>,
    id: String,
    dest_zip: String,
) -> Result<u64, String> {
    let current = workspace::cmds::current(&state.registry_path()).map_err(|e| e.to_string())?;
    if current.as_ref().map(|w| w.id.as_str()) != Some(id.as_str()) {
        return Err("只能备份当前工作空间".to_string());
    }
    let entry = workspace::cmds::list(&state.registry_path())
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|w| w.id == id)
        .ok_or_else(|| "工作空间不存在".to_string())?;
    let layout = workspace::layout::Layout::at(Path::new(&entry.path));
    state
        .with_conn(|conn| workspace::backup::backup(&layout, conn, Path::new(&dest_zip)))
        .map_err(|e| e.to_string())
}

/// 转移：复制（跳过 cache/）→ 校验签名 → 注册表 relocate → switch_db → 删源目录。
/// 复制/校验失败时注册表与源目录均未动；relocate 成功后、删源前任何失败只会
/// 留下「注册表已指向新路径」的不一致（数据完整，可手动重试或清理），
/// 故注册表更新先于删源，保证任何时刻数据都可被发现。
#[tauri::command]
fn workspace_transfer_cmd(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
    dest_root: String,
) -> Result<(), String> {
    let entry = workspace::cmds::list(&state.registry_path())
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|w| w.id == id)
        .ok_or_else(|| "工作空间不存在".to_string())?;
    let src_root = PathBuf::from(&entry.path);
    let dest = PathBuf::from(&dest_root);
    // 目标目录必须不存在或为空（前端弹确认后可改用其下子目录重试）
    workspace::layout::ensure_empty_dest(&dest).map_err(|e| e.to_string())?;
    // 1) 复制 + 校验（失败 → 提前返回，原目录与注册表均未动）
    workspace::backup::transfer(&src_root, &dest).map_err(|e| e.to_string())?;
    // 2) 注册表 path 更新（先于删源；此后失败仅路径变化，数据在 dest 完整）
    workspace::cmds::relocate(&state.registry_path(), &id, &dest).map_err(|e| e.to_string())?;
    // 3) DB 重连到新位置
    state.switch_db(&entry_path_db(&dest)).map_err(|e| e.to_string())?;
    // 4) 清理源目录
    std::fs::remove_dir_all(&src_root)
        .map_err(|e| format!("转移完成但清理源目录失败：{e}"))?;
    events::emit_push_update(&app, 0);
    Ok(())
}

/// 首次引导：创建第一个工作空间 + 切换到其 DB；全新空库补建默认欢迎便签。
#[tauri::command]
fn workspace_bootstrap_cmd(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    path: String,
    name: Option<String>,
) -> Result<models::WorkspaceEntryDto, String> {
    let root = PathBuf::from(&path);
    // 首次引导同样要求目标目录不存在或为空（OnboardingDialog 处理非空确认流）
    workspace::layout::ensure_empty_dest(&root).map_err(|e| e.to_string())?;
    let entry = workspace::cmds::create(&state.registry_path(), &root, name.as_deref())
        .map_err(|e| e.to_string())?;
    let db_path = entry_path_db(&root);
    state.switch_db(&db_path).map_err(|e| e.to_string())?;
    // 全新工作空间：补建默认欢迎便签（md 主存储随 create 一并落盘）。
    let stickers = state.with_conn(commands::list_stickers).unwrap_or_default();
    if stickers.is_empty() {
        state
            .with_conn_path(|c, db| commands::create_welcome_sticker(c, db))
            .map_err(|e| format!("创建默认欢迎便签失败：{e}"))?;
        tracing::info!("[bootstrap] 全新工作空间，已创建默认欢迎便签");
    }
    events::emit_push_update(&app, 0); // 主控台刷新
    Ok(workspace_to_dto(entry))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 初始化 tracing 日志：默认 debug 级别，输出详细操作/事件日志；
    // 可用 RUST_LOG 环境变量覆盖。
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("debug")),
        )
        .try_init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        // 开机自启：三平台统一由官方插件管理（Windows 注册表 Run 键 /
        // macOS LaunchAgent / Linux XDG autostart）；携带 --autostart 参数，
        // 供程序识别"由登录自启拉起"，避免每次登录都弹出主控台窗口。
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![crate::AUTOSTART_ARG]),
        ))
        .setup(|app| {
            // 开机自启拉起（--autostart）：主控台窗口不弹出（托盘仍在，可随时呼出），
            // 便签窗口照常按上次会话恢复——这正是"开机即见便签"的预期体验。
            let from_autostart = std::env::args().any(|a| a == crate::AUTOSTART_ARG);
            if from_autostart {
                if let Some(main_win) = app.get_webview_window("main") {
                    let _ = main_win.hide();
                }
                tracing::info!("[setup] 由开机自启拉起（{}），主控台已隐藏", crate::AUTOSTART_ARG);
            }

            // 数据层初始化：解析注册表中的有效工作控件并打开其数据库。
            // 无任何有效工作控件时使用内存占位库（不落盘）——前端首次引导
            // 创建真实工作空间后再 switch_db 切换；启动路径绝不无声重建目录。
            let app_data_dir = app
                .path()
                .app_data_dir()
                .map_err(|e| format!("获取 app_data_dir 失败: {e}"))?;
            // 程序级目录只保留 workspaces.json：清掉旧版单库与 bootstrap 临时库。
            workspace::cleanup_legacy_artifacts(&app_data_dir);
            let boot = workspace::resolve_boot_workspace(&app_data_dir)
                .map_err(|e| format!("解析工作空间注册表失败: {e}"))?;
            for name in &boot.removed_invalid {
                tracing::warn!("[setup] 工作空间目录已失效，已从注册表移除：{name}");
            }
            let (conn, config, db_path) = match &boot.db_path {
                Some(db) => {
                    let conn = db::connection::open(db)
                        .map_err(|e| format!("打开数据库失败: {e}"))?;
                    db::schema::run_migrations(&conn)
                        .map_err(|e| format!("数据库迁移失败: {e}"))?;
                    let config = db::config_repo::load_all(&conn)
                        .map_err(|e| format!("读取配置失败: {e}"))?;
                    (conn, config, db.to_string_lossy().into_owned())
                }
                None => {
                    tracing::info!("[setup] 无有效工作空间，使用内存占位库等待首次引导");
                    let conn = rusqlite::Connection::open_in_memory()
                        .map_err(|e| format!("打开占位数据库失败: {e}"))?;
                    db::schema::run_migrations(&conn)
                        .map_err(|e| format!("数据库迁移失败: {e}"))?;
                    let config = db::config_repo::load_all(&conn)
                        .map_err(|e| format!("读取配置失败: {e}"))?;
                    (conn, config, ":memory:".to_string())
                }
            };
            app.manage(AppState::new(
                conn,
                config,
                db_path,
                app_data_dir.to_string_lossy().into_owned(),
            ));

            let handle = app.handle().clone();
            let state = app.state::<AppState>().inner().clone();

            // 启动一致性：为正文中缺少标记的孤儿 Todo 块补写标记（旧版本遗留），
            // 并通知所属便签窗口刷新正文。
            if let Ok(retagged) = state.with_conn(commands::retag_orphan_todos) {
                for (sticker_id, todo_ids) in &retagged {
                    tracing::info!("[setup] 为孤儿 Todo 块补写标记 sticker={sticker_id} ids={todo_ids:?}");
                    events::emit_push_update(&handle, *sticker_id);
                }
            }

            // 系统托盘（新建便签/打开主控台/系统设置/退出）
            platform::tray::install(&handle, dispatch_tray)?;

            // 全局鼠标钩子：display 全穿透 + 右键双击唤醒
            platform::mouse_hook::install(&handle)?;

            // 启动恢复：为数据库中已有便签重建窗口；空库则创建默认展示便签。
            // 内存占位库（尚无注册工作空间，等待首次引导）不创建欢迎便签，
            // 避免孤儿便签卡在占位数据上。
            let stickers = state
                .with_conn(commands::list_stickers)
                .unwrap_or_default();
            let on_placeholder_db = boot.db_path.is_none();
            if stickers.is_empty() {
                if on_placeholder_db {
                    tracing::info!("[setup] 启动于内存占位库（未注册工作空间），跳过默认便签创建");
                } else {
                    // 真实工作空间空库：创建一条默认便签，便于查看效果
                    let default = commands::welcome_sticker_new();
                    if let Ok(id) = state.with_conn_path(|c, db| create_sticker(c, &default, db)) {
                        // 首次默认便签按系统默认置顶（default_sticker_always_on_top=1）
                        let args = StickerWinArgs {
                            always_on_top: true,
                            ..StickerWinArgs::from_new(&default, id)
                        };
                        let _ = create_sticker_win(&handle, args);
                    }
                }
            } else {
                for s in stickers {
                    // 上次退出时被关闭（隐藏）的便签：不创建窗口，保持关闭状态；
                    // 需要时由主控台"显示"按钮经 wake_sticker_cmd 按记录重建。
                    if s.window_hidden {
                        tracing::info!("[setup] 便签 #{} 上次为隐藏状态，跳过窗口创建", s.id);
                        continue;
                    }
                    if let Ok(win) = create_sticker_win(&handle, StickerWinArgs::from_sticker(&s)) {
                        // 启动即按持久化模式同步窗口状态（display → 穿透+锁尺寸），
                        // 不依赖前端加载完成，确保展示模式立即生效。
                        if s.display_mode == "display" {
                            if let Ok(size) = win.outer_size() {
                                let _ = win.set_min_size(Some(size));
                                let _ = win.set_max_size(Some(size));
                            }
                            let _ = win.set_resizable(false);
                            let _ = win.set_ignore_cursor_events(true);
                            state.set_display_window(s.id, true);
                            tracing::info!("[setup] 便签 #{} 以展示模式恢复（穿透+锁尺寸）", s.id);
                        } else {
                            let _ = win.set_ignore_cursor_events(false);
                            state.set_display_window(s.id, false);
                        }
                    }
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_stickers_cmd,
            get_sticker_cmd,
            create_sticker_cmd,
            update_sticker_cmd,
            delete_sticker_cmd,
            group_list_cmd,
            group_create_cmd,
            group_rename_cmd,
            group_delete_cmd,
            move_sticker_group_cmd,
            fetch_page_title_cmd,
            get_config_cmd,
            set_config_cmd,
            update_sticker_prefs_cmd,
            reset_sticker_prefs_cmd,
            effective_prefs_cmd,
            toggle_todo_cmd,
            get_todo_block_cmd,
            list_todo_for_sticker_cmd,
            create_todo_block_cmd,
            update_todo_block_cmd,
            delete_todo_block_cmd,
            sync_todo_marker_cmd,
            notify_todo_presence_cmd,
            reorder_todo_cmd,
            open_todo_window_cmd,
            close_todo_window_cmd,
            open_external_cmd,
            check_update_cmd,
            autostart_get_cmd,
            autostart_set_cmd,
            main_close_cmd,
            apply_window_state_cmd,
            reset_sticker_window_cmd,
            wake_sticker_cmd,
            list_open_sticker_ids_cmd,
            hide_sticker_cmd,
            slash_query_cmd,
            workspace_list_cmd,
            workspace_current_cmd,
            workspace_create_cmd,
            workspace_switch_cmd,
            workspace_destroy_cmd,
            workspace_default_path_cmd,
            workspace_backup_cmd,
            workspace_transfer_cmd,
            workspace_bootstrap_cmd
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // 退出前（含窗口仍存活时）保存所有便签的显示/隐藏与位置尺寸，
            // 供下次启动恢复"上次显示的那些便签"及其几何。
            match event {
                tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit => {
                    persist_sticker_window_states(app);
                }
                _ => {}
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 启动解析：失效工作控件被移除并回退到有效项；全部失效时返回 None。
    #[test]
    fn resolve_boot_workspace_removes_invalid_and_falls_back() {
        let dir = std::env::temp_dir().join(format!("boot-resolve-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // 有效控件 a（含 data/index.db）；失效控件 b（目录不存在）
        let root_a = dir.join("ws-a");
        workspace::layout::ensure_layout(&workspace::layout::Layout::at(&root_a), "A").unwrap();
        std::fs::create_dir_all(root_a.join("data")).unwrap();
        std::fs::write(root_a.join("data").join("index.db"), b"").unwrap();
        let reg_path = dir.join("workspaces.json");
        let reg = workspace::layout::Registry {
            current: Some("w-b".into()),
            workspaces: vec![
                workspace::layout::WorkspaceEntry { id: "w-a".into(), name: "A".into(), path: root_a.to_string_lossy().into_owned(), created_at: "1".into() },
                workspace::layout::WorkspaceEntry { id: "w-b".into(), name: "B".into(), path: dir.join("ws-b").to_string_lossy().into_owned(), created_at: "2".into() },
            ],
        };
        workspace::layout::save_registry(&reg_path, &reg).unwrap();

        let boot = workspace::resolve_boot_workspace(&dir).unwrap();
        assert_eq!(boot.db_path.as_deref(), Some(root_a.join("data").join("index.db")).as_deref());
        assert_eq!(boot.removed_invalid.len(), 1);
        // 注册表已清理且 current 回退到有效项
        let after = workspace::layout::load_registry(&reg_path).unwrap();
        assert_eq!(after.workspaces.len(), 1);
        assert_eq!(after.current.as_deref(), Some("w-a"));

        // 全部失效 → None + 注册表清空
        let _ = std::fs::remove_dir_all(&root_a);
        let boot = workspace::resolve_boot_workspace(&dir).unwrap();
        assert!(boot.db_path.is_none());
        assert_eq!(boot.removed_invalid.len(), 1);
        let after = workspace::layout::load_registry(&reg_path).unwrap();
        assert!(after.workspaces.is_empty() && after.current.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 遗留数据清扫：stickers.db 与 bootstrap 目录被删除，workspaces.json 保留。
    #[test]
    fn cleanup_legacy_artifacts_only_touches_known_files() {
        let dir = std::env::temp_dir().join(format!("cleanup-legacy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("bootstrap")).unwrap();
        std::fs::write(dir.join("stickers.db"), b"x").unwrap();
        std::fs::write(dir.join("stickers.db-wal"), b"x").unwrap();
        std::fs::write(dir.join("workspaces.json"), b"{}").unwrap();

        workspace::cleanup_legacy_artifacts(&dir);

        assert!(!dir.join("stickers.db").exists());
        assert!(!dir.join("stickers.db-wal").exists());
        assert!(!dir.join("bootstrap").exists());
        assert!(dir.join("workspaces.json").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// HTML <title> 提取：去标签/实体/空白，空标题返回 None。
    #[test]
    fn extract_page_title_parses_html() {
        assert_eq!(
            extract_page_title("<html><head><title>  哔哩哔哩 (゜-゜)つロ 干杯~-bilibili </title></head></html>").as_deref(),
            Some("哔哩哔哩 (゜-゜)つロ 干杯~-bilibili")
        );
        assert_eq!(extract_page_title("<title>A &amp; B</title>").as_deref(), Some("A & B"));
        assert_eq!(extract_page_title("no title here"), None);
        assert_eq!(extract_page_title("<title></title>"), None);
        assert_eq!(extract_page_title("<TITLE>Hello</TITLE>").as_deref(), Some("Hello"));
    }

    /// 自启动平台信息：按 target_os 编译生成，描述非空且与各平台机制对应。
    #[test]
    fn autostart_platform_info_described() {
        assert!(!autostart_platform().is_empty());
        assert!(!autostart_mechanism().is_empty());
        match autostart_platform() {
            "windows" => assert!(autostart_mechanism().contains("Run"), "Windows 应为注册表 Run 键机制"),
            "macos" => assert!(autostart_mechanism().contains("LaunchAgent"), "macOS 应为 LaunchAgent 机制"),
            "linux" => assert!(autostart_mechanism().contains("autostart"), "Linux 应为 XDG autostart 机制"),
            other => panic!("未预期平台：{other}"),
        }
    }
}

