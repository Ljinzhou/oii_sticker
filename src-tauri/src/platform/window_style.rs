//! 窗口样式封装：透明/无边框/置顶/任务栏隐藏/点击穿透/拖动。
//!
//! 语义平移自旧项目 `winit_bridge.rs`，全部改用 Tauri `Window` API。

use tauri::WebviewWindow;

/// 应用便签窗口基础样式（透明/无边框由创建时 Builder 设置，此处补充
/// 运行时属性：任务栏隐藏、置顶）。
pub fn apply_sticker_style(win: &WebviewWindow, skip_taskbar: bool, always_on_top: bool) {
    let _ = win.set_skip_taskbar(skip_taskbar);
    let _ = win.set_always_on_top(always_on_top);
}

/// 切换点击穿透：`enabled == true` 表示窗口忽略鼠标事件（display 模式），
/// `false` 表示窗口捕获事件（edit 模式）。
pub fn set_click_passthrough(win: &WebviewWindow, enabled: bool) {
    let _ = win.set_ignore_cursor_events(enabled);
}

/// 开始 OS 级窗口拖动（无边框窗口标题栏 pressed 回调中调用）。
pub fn start_drag(win: &WebviewWindow) {
    let _ = win.start_dragging();
}
