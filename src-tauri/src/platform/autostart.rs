//! 开机自启封装：`tauri-plugin-autostart`（官方插件，替代注册表手搓）。
//! 非 Windows 平台由插件自行处理（macOS LaunchAgent / Linux autostart）。

use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

/// 查询自启是否已启用。
pub fn is_enabled(app: &AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

/// 启用开机自启。
pub fn enable(app: &AppHandle) -> Result<(), String> {
    app.autolaunch()
        .enable()
        .map_err(|e| format!("启用自启失败：{e}"))
}

/// 禁用开机自启。
pub fn disable(app: &AppHandle) -> Result<(), String> {
    app.autolaunch()
        .disable()
        .map_err(|e| format!("禁用自启失败：{e}"))
}
