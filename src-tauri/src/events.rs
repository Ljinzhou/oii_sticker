//! 事件名常量 + emit 封装（替代旧项目的 WindowSink 闭包桥）。

use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// 便签内容/属性变更推送（label = `sticker-<id>` 或 `main`）。
pub const PUSH_UPDATE: &str = "sticky://push-update";
/// 偏好变更。
pub const PREFS_UPDATED: &str = "sticky://prefs-updated";
/// 请求打开设置面板（前端监听）。
pub const OPEN_SETTINGS: &str = "sticky://open-settings";
/// Todo 块变更，定向发给所属便签和对应 Todo 窗口。
pub const TODO_UPDATED: &str = "todo://updated";
/// Todo 提醒触发（到点/截止），广播给所有窗口弹应用内提示。
pub const TODO_REMINDER: &str = "todo://reminder-fired";
/// 父便签窗口/便签已消失：通知其 Todo 编辑窗口自动保存并关闭（按 sticker_id 过滤）。
pub const STICKER_CLOSED: &str = "sticky://sticker-closed";

/// 向指定窗口 label 发事件。
pub fn emit_to_label(app: &AppHandle, label: &str, event: &str, payload: impl Serialize + Clone) {
    tracing::debug!("[event] emit_to label={label} event={event}");
    let _ = app.emit_to(label, event, payload);
}

/// 便签内容变更推送（主控台与便签窗口都收到）。
pub fn emit_push_update(app: &AppHandle, sticker_id: i64) {
    tracing::debug!("[event] push_update sticker={sticker_id}");
    let _ = app.emit(PUSH_UPDATE, sticker_id);
}

pub fn emit_todo_updated(app: &AppHandle, sticker_id: i64, todo_id: &str) {
    emit_to_label(app, &format!("sticker-{sticker_id}"), TODO_UPDATED, todo_id);
    emit_to_label(app, &format!("todo-{todo_id}"), TODO_UPDATED, todo_id);
    let _ = app.emit(TODO_UPDATED, todo_id);
}

/// 广播提醒触发事件（所有窗口均可收到，自行按 sticker_id 过滤展示）。
pub fn emit_todo_reminder(app: &AppHandle, ctx: &crate::reminder::FireContext) {
    tracing::info!("[event] todo_reminder todo={} kind={:?}", ctx.id, ctx.kind);
    let _ = app.emit(TODO_REMINDER, ctx);
}

/// 广播「父便签已关闭/已删除」（Todo 窗口按自身 sticker_id 过滤后自动保存并关闭）。
///
/// 触发点只有一处：便签窗口被真正销毁时（`RunEvent::WindowEvent::Destroyed`）。
/// 正常关闭路径已被 `ensure_no_open_todo` 拦截，这里是**兜底**——外部删除 md、
/// Alt+F4/任务栏强关、切库、程序退出等任何让父便签消失的路径都能覆盖到。
pub fn emit_sticker_closed(app: &AppHandle, sticker_id: i64) {
    tracing::info!("[event] sticker_closed sticker={sticker_id}");
    let _ = app.emit(STICKER_CLOSED, sticker_id);
}
