//! 系统托盘：TrayIconBuilder + 菜单（新建便签 / 打开主控台 / 系统设置 / 退出）。
//!
//! 语义平移自旧项目 `tray.rs`：
//! - 菜单事件 → 对应动作（新建便签 / 显示主控台 / 显示设置 / 退出）；
//! - 左键单击/双击托盘图标 → 打开主控台；
//! - 图标：打包的应用图标（双层便签 A2），随 bundle.icon 嵌入运行时。

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
use tauri::AppHandle;

/// 托盘菜单动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    NewSticker,
    OpenMain,
    OpenSettings,
    Quit,
}

/// 安装系统托盘。`on_action` 回调分发动作（由调用方实现具体行为）。
pub fn install(app: &AppHandle, on_action: impl Fn(&AppHandle, TrayAction) + Send + Sync + 'static) -> tauri::Result<()> {
    // 菜单事件与图标事件两个闭包共享回调：Arc 包装 + 预克隆。
    let on_action = std::sync::Arc::new(on_action);
    let on_menu = on_action.clone();
    let on_tray = on_action.clone();

    // 构建菜单
    let new_sticker = MenuItem::with_id(app, "new_sticker", "新建便签", true, None::<&str>)?;
    let open_main = MenuItem::with_id(app, "open_main", "打开主控台", true, None::<&str>)?;
    let open_settings = MenuItem::with_id(app, "open_settings", "系统设置", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "退出程序", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&new_sticker, &open_main, &open_settings, &separator, &quit])?;

    // 托盘图标：优先取随构建嵌入的应用图标（tauri.conf.json bundle.icon），
    // 缺失时退回内嵌的 icons/32x32.png；纯绿 RGBA 仅为最后兜底（旧项目同款，理论不可达）。
    let icon = app
        .default_window_icon()
        .cloned()
        .or_else(|| {
            tauri::image::Image::from_bytes(include_bytes!("../../icons/32x32.png")).ok()
        })
        .unwrap_or_else(|| {
            let mut buf = Vec::with_capacity(32 * 32 * 4);
            for _ in 0..(32 * 32) {
                buf.extend_from_slice(&[0xCC, 0xFF, 0xCC, 0xFF]);
            }
            tauri::image::Image::new_owned(buf, 32, 32)
        });

    TrayIconBuilder::with_id("main-tray")
        .tooltip("Oi Sticker")
        .icon(icon)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| {
            let on_action = on_menu.clone();
            let action = match event.id().as_ref() {
                "new_sticker" => Some(TrayAction::NewSticker),
                "open_main" => Some(TrayAction::OpenMain),
                "open_settings" => Some(TrayAction::OpenSettings),
                "quit" => Some(TrayAction::Quit),
                _ => None,
            };
            if let Some(action) = action {
                on_action(app, action);
            }
        })
        .on_tray_icon_event(move |tray, event| {
            let on_action = on_tray.clone();
            let is_left = matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    ..
                } | TrayIconEvent::DoubleClick { .. }
            );
            if is_left {
                on_action(tray.app_handle(), TrayAction::OpenMain);
            }
        })
        .build(app)?;

    tracing::info!("系统托盘已安装");
    Ok(())
}
