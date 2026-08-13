//! 全局低级鼠标钩子（WH_MOUSE_LL）：实现"display 全穿透 + 右键双击唤醒"。
//!
//! 背景：`set_ignore_cursor_events(true)` 后窗口收不到任何鼠标事件，
//! 右键双击也无法触发。为满足"展示模式全穿透，右键双击除外"的需求，
//! 在**主线程**安装 WH_MOUSE_LL 钩子（tao 事件循环会派发钩子消息），
//! 在回调中检测：光标位于某个 display 模式便签窗口内 + 右键双击 →
//! 取消穿透并切换 interact（经 `sticky://wake` 事件通知前端）。
//!
//! 注：这是用户明确批准引入的 windows-sys 直调（唯一无钩子替代方案）。

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter, Manager};
use windows_sys::Win32::Foundation::{LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, MSLLHOOKSTRUCT, WH_MOUSE_LL,
    WM_RBUTTONDOWN,
};

use crate::db::sticker_repo::StickerPatch;
use crate::state::AppState;

/// 右键双击判定参数。
const DBL_CLICK_MS: i64 = 350;
const DBL_CLICK_DIST: i64 = 10;

struct HookInner {
    app: AppHandle,
    /// 上次右键按下时间与位置（双击检测）。
    last_down: Option<(i64, POINT)>,
}

static STATE: Mutex<Option<HookInner>> = Mutex::new(None);
/// HHOOK 是指针，static 中存 usize 避免 Send/Sync 问题。
static HOOK: Mutex<Option<usize>> = Mutex::new(None);

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn dist(a: POINT, b: POINT) -> i64 {
    let dx = (a.x - b.x) as i64;
    let dy = (a.y - b.y) as i64;
    (dx * dx + dy * dy).isqrt()
}

/// 命中检测：光标是否落在某个 display 模式便签窗口内；命中则唤醒。
fn try_wake(app: &AppHandle, pt: POINT) {
    let state = app.state::<AppState>();
    let display_ids: Vec<i64> = state.display_windows().iter().copied().collect();
    for id in display_ids {
        let Some(win) = app.get_webview_window(&format!("sticker-{id}")) else {
            continue;
        };
        let Ok(pos) = win.outer_position() else { continue };
        let size = win.outer_size().unwrap_or_default();
        let inside = pt.x >= pos.x
            && pt.x <= pos.x + size.width as i32
            && pt.y >= pos.y
            && pt.y <= pos.y + size.height as i32;
        if !inside {
            continue;
        }
        // 唤醒：取消穿透 + 恢复 resize + 写库 interact + 通知前端切换
        let _ = win.set_ignore_cursor_events(false);
        let _ = win.set_resizable(true);
        let _ = win.set_min_size(None::<tauri::Size>);
        let _ = win.set_max_size(None::<tauri::Size>);
        let _ = win.set_focus();
        state.remove_display_window(id);
        let _ = state.with_conn(|c| {
            crate::commands::update_sticker(
                c,
                id,
                &StickerPatch {
                    display_mode: Some("interact".into()),
                    ..Default::default()
                },
            )
        });
        let _ = app.emit_to(format!("sticker-{id}"), "sticky://wake", ());
        tracing::info!("[hook] 右键双击唤醒便签 #{id}");
        return;
    }
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && wparam == WM_RBUTTONDOWN as usize {
        let info = &*(lparam as *const MSLLHOOKSTRUCT);
        let mut state = STATE.lock().unwrap();
        if let Some(inner) = state.as_mut() {
            let now = now_ms();
            let is_dbl = inner
                .last_down
                .map(|(t, p)| now - t < DBL_CLICK_MS && dist(p, info.pt) < DBL_CLICK_DIST)
                .unwrap_or(false);
            if is_dbl {
                inner.last_down = None;
                try_wake(&inner.app, info.pt);
            } else {
                inner.last_down = Some((now, info.pt));
            }
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

/// 安装全局鼠标钩子（必须在主线程调用——setup 中执行）。
pub fn install(app: &AppHandle) -> Result<(), String> {
    *STATE.lock().unwrap() = Some(HookInner {
        app: app.clone(),
        last_down: None,
    });
    // 钩子回调需要目标线程有消息循环：Tauri 主线程的 tao 事件循环满足。
    let hook = unsafe {
        SetWindowsHookExW(WH_MOUSE_LL, Some(hook_proc), std::ptr::null_mut(), 0)
    };
    if hook.is_null() {
        *STATE.lock().unwrap() = None;
        return Err("安装 WH_MOUSE_LL 钩子失败".into());
    }
    *HOOK.lock().unwrap() = Some(hook as usize);
    tracing::info!("[hook] 全局鼠标钩子已安装（右键双击唤醒穿透便签）");
    Ok(())
}

/// 卸载钩子（进程退出时调用，非必须——进程结束钩子自动移除）。
pub fn uninstall() {
    if let Some(hook) = HOOK.lock().unwrap().take() {
        unsafe {
            UnhookWindowsHookEx(hook as HHOOK);
        }
    }
    *STATE.lock().unwrap() = None;
}
