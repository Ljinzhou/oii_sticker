//! 事件名常量 + emit 封装（替代旧项目的 WindowSink 闭包桥）。

use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// 便签内容/属性变更推送（label = `sticker-<id>` 或 `main`）。
pub const PUSH_UPDATE: &str = "sticky://push-update";
/// 提醒触发状态信号（payload: bool，无动画，前端自行表现）。
pub const ALERT_ACTIVE: &str = "sticky://alert-active";
/// 偏好变更。
pub const PREFS_UPDATED: &str = "sticky://prefs-updated";
/// 请求打开设置面板（前端监听）。
pub const OPEN_SETTINGS: &str = "sticky://open-settings";

/// 向指定窗口 label 发事件。
pub fn emit_to_label(app: &AppHandle, label: &str, event: &str, payload: impl Serialize + Clone) {
    tracing::debug!("[event] emit_to label={label} event={event}");
    let _ = app.emit_to(label, event, payload);
}

/// 提醒触发/结束状态信号（按便签窗口 label）。
pub fn emit_alert_active(app: &AppHandle, sticker_id: i64, active: bool) {
    tracing::debug!("[event] alert_active sticker={sticker_id} active={active}");
    let _ = app.emit_to(
        format!("sticker-{sticker_id}"),
        ALERT_ACTIVE,
        active,
    );
}

/// 便签内容变更推送（主控台与便签窗口都收到）。
pub fn emit_push_update(app: &AppHandle, sticker_id: i64) {
    tracing::debug!("[event] push_update sticker={sticker_id}");
    let _ = app.emit(PUSH_UPDATE, sticker_id);
}
