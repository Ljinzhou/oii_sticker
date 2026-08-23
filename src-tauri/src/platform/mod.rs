//! 平台集成：托盘、通知、自启、窗口样式、鼠标钩子（全部 Tauri 原生 API / 官方插件；
//! 鼠标钩子为用户批准的唯一 windows-sys 直调）。

pub mod autostart;
pub mod mouse_hook;
pub mod tray;
pub mod window_style;
