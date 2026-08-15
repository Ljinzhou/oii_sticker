//! 全局低级鼠标钩子（WH_MOUSE_LL）：实现"display 全穿透 + 中键+左键唤醒"。
//!
//! 背景：`set_ignore_cursor_events(true)` 后窗口收不到任何鼠标事件。
//! 唤醒协议（用户定义）：用户任意位置按下一次**鼠标中键**（武装），
//! 随后**左键点击**任意展示模式便签 → 该便签取消穿透并进入交互模式。
//!
//! 在**主线程**安装 WH_MOUSE_LL 钩子（tao 事件循环会派发钩子消息），
//! 回调中：中键按下 → 记录武装时间（3 秒有效）；左键按下且已武装 →
//! 命中检测并唤醒。
//!
//! 注：这是用户明确批准引入的 windows-sys 直调（唯一无钩子替代方案）。

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter, Manager};
use windows_sys::Win32::Foundation::{LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, MSLLHOOKSTRUCT, WH_MOUSE_LL,
    WM_LBUTTONDOWN, WM_MBUTTONDOWN,
};

use crate::db::sticker_repo::StickerPatch;
use crate::state::AppState;

/// 中键武装后左键唤醒的有效期（毫秒）。
const ARM_TIMEOUT_MS: i64 = 3000;

struct HookInner {
    app: AppHandle,
    /// 中键按下时刻（武装）；None = 未武装。
    armed_at: Option<i64>,
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

/// 命中检测：光标是否落在某个 display 模式便签窗口内；命中则唤醒。
fn try_wake(app: &AppHandle, pt: POINT) {
    let state = app.state::<AppState>();
    let display_ids: Vec<i64> = state.display_windows().iter().copied().collect();
    tracing::debug!(
        "[hook] try_wake: display_ids={:?} cursor=({},{})",
        display_ids,
        pt.x,
        pt.y
    );
    for id in display_ids {
        let Some(win) = app.get_webview_window(&format!("sticker-{id}")) else {
            tracing::debug!("[hook] try_wake: sticker-{id} 窗口不存在");
            continue;
        };
        let Ok(pos) = win.outer_position() else { continue };
        let size = win.outer_size().unwrap_or_default();
        tracing::debug!(
            "[hook] try_wake: sticker-{id} pos=({},{}) size=({},{})",
            pos.x,
            pos.y,
            size.width,
            size.height
        );
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
        let _ = app.emit_to(format!("sticker-{id}"), "sticky://wake", id);
        tracing::info!("[hook] 中键+左键唤醒便签 #{id}");
        return;
    }
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let info = &*(lparam as *const MSLLHOOKSTRUCT);
        let now = now_ms();
        // ⚠ 性能红线：WH_MOUSE_LL 回调运行在系统鼠标消息线程，回调阻塞会
        // 卡住**整个系统**的鼠标输入（表现为鼠标无法移动、程序未响应）。
        // 因此回调内只做轻量状态记录（短锁），所有重活（窗口操作/写库/
        // 事件）一律异步投递到主线程执行，回调必须立即返回。
        match wparam as u32 {
            // 中键按下：武装（3 秒内左键点击便签即唤醒）
            WM_MBUTTONDOWN => {
                if let Ok(mut state) = STATE.lock() {
                    if let Some(inner) = state.as_mut() {
                        inner.armed_at = Some(now);
                    }
                }
                tracing::debug!("[hook] 中键按下，武装唤醒（3 秒内左键点击展示便签）");
            }
            // 左键按下：若已武装（未超时）→ 投递主线程执行唤醒
            WM_LBUTTONDOWN => {
                let armed = STATE.lock().ok().and_then(|mut s| {
                    let inner = s.as_mut()?;
                    let armed = inner
                        .armed_at
                        .map(|t| now - t < ARM_TIMEOUT_MS)
                        .unwrap_or(false);
                    inner.armed_at = None; // 一次武装对应一次左键
                    Some(armed)
                });
                if armed.unwrap_or(false) {
                    // 异步投递：回调绝不阻塞系统鼠标线程
                    if let Ok(state) = STATE.lock() {
                        if let Some(inner) = state.as_ref() {
                            let app = inner.app.clone();
                            let pt = info.pt;
                            let app2 = app.clone();
                            let _ = app.run_on_main_thread(move || try_wake(&app2, pt));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

/// 安装全局鼠标钩子（必须在主线程调用——setup 中执行）。
pub fn install(app: &AppHandle) -> Result<(), String> {
    *STATE.lock().unwrap() = Some(HookInner {
        app: app.clone(),
        armed_at: None,
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
    tracing::info!("[hook] 全局鼠标钩子已安装（中键武装+左键点击唤醒穿透便签）");
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
