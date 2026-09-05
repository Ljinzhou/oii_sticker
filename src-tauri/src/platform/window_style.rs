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

/// 便签窗口「任务栏隐藏 × 置顶」的计算结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StickerWindowPolicy {
    pub skip_taskbar: bool,
    pub always_on_top: bool,
}

/// 计算便签窗口的任务栏隐藏与置顶策略：
/// - 编辑模式（`is_edit == true`）：是否隐藏任务栏由 `edit_hide` 决定，且恒取消置顶；
/// - 非编辑模式（显示/交互）：是否隐藏任务栏由 `default_hide` 决定，置顶恢复为便签
///   自身的 `restore_top`（编辑前的置顶值）。
pub fn sticker_window_policy(
    default_hide: bool,
    edit_hide: bool,
    is_edit: bool,
    restore_top: bool,
) -> StickerWindowPolicy {
    if is_edit {
        StickerWindowPolicy {
            skip_taskbar: edit_hide,
            always_on_top: false,
        }
    } else {
        StickerWindowPolicy {
            skip_taskbar: default_hide,
            always_on_top: restore_top,
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_mode_shows_in_taskbar_and_drops_topmost() {
        let p = sticker_window_policy(true, false, true, true);
        assert!(!p.skip_taskbar, "编辑模式下应显示在任务栏");
        assert!(!p.always_on_top, "编辑模式下应取消置顶");
    }

    #[test]
    fn edit_mode_hide_wins_when_enabled() {
        let p = sticker_window_policy(false, true, true, true);
        assert!(p.skip_taskbar, "编辑隐藏开关开启时编辑模式仍隐藏任务栏");
        assert!(!p.always_on_top, "编辑模式恒取消置顶");
    }

    #[test]
    fn non_edit_restores_default_hide_and_topmost() {
        let p = sticker_window_policy(true, false, false, true);
        assert!(p.skip_taskbar, "非编辑模式恢复默认隐藏");
        assert!(p.always_on_top, "非编辑模式恢复置顶");
    }

    #[test]
    fn non_edit_with_hide_disabled_shows_in_taskbar() {
        let p = sticker_window_policy(false, false, false, false);
        assert!(!p.skip_taskbar, "默认隐藏关闭时非编辑模式显示在任务栏");
        assert!(!p.always_on_top, "便签自身未置顶则保持非置顶");
    }
}
