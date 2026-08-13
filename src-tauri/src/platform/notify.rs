//! 系统通知封装：`tauri-plugin-notification`（官方插件，替代 notify-rust）。

use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

/// 发送系统通知。失败仅记录日志，不影响主流程。
pub fn send(app: &AppHandle, title: &str, body: &str) {
    let result = app
        .notification()
        .builder()
        .title(title)
        .body(body)
        .show();
    if let Err(e) = result {
        tracing::warn!("[提醒] 系统通知失败：{e}");
    }
}
