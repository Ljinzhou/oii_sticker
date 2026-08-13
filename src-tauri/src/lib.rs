use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

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
            list_sticker_windows
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
