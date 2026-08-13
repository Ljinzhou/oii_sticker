# 阶段 3 — 平台集成（tray / notify / autostart / window_style）

> 所属总览：[`../PLAN.md`](../PLAN.md) · 状态：⏳ 未开始 · 前置：阶段 2 完成

## 3️⃣ 需要的 Skills / MCP（本阶段）
| 工具 | 用途 | 状态 |
|---|---|---|
| `context7-mcp`（MCP） | Tauri 2 精确 API：`TrayIconBuilder` + `Menu`、`tauri-plugin-notification`（`Notification::new().show()`）、`tauri-plugin-autostart`（ManagerExt/`AutoStartManager`）、`Window` API（`set_skip_taskbar`/`set_always_on_top`/`set_ignore_cursor_events`/`start_dragging`） | ✅ 已装 |
| `Ref`（MCP） | Tauri 官方文档/plugins-workspace 仓库核对配置与权限 | ✅ 已装 |
| `explore`（子代理） | 提取旧项目意图规格：`tray.rs` 菜单结构与事件语义、`tools/win32.rs` 自启行为、`winit_bridge.rs` 窗口操作清单（仅提取"意图"，不迁代码） | ✅ 已装 |
| `security_review` | 平台集成涉及系统级能力，完成后来一轮 | ✅ 已装 |

## 📋 详细步骤
1. `src-tauri/src/platform/tray.rs`：`TrayIconBuilder` + `Menu`（新建便签/打开主控台/系统设置/退出），`on_menu_event` 映射命令，`on_tray_icon_event`（单击显示主控台）；图标用 `Image::from_rgba` 运行时生成（源项目同为运行时纯色图标）或放 PNG 资源
2. `platform/notify.rs`：封装 `tauri-plugin-notification`（标题/正文/`appname: Oi Sticker` 语义平移）
3. `platform/autostart.rs`：封装 `tauri-plugin-autostart`（`is_enabled`/`enable`/`disable`），非 Windows 空操作
4. `platform/window_style.rs`：按 `display_mode`/偏好应用 `set_skip_taskbar`/`set_always_on_top`/`set_ignore_cursor_events`/`set_opacity`；`tauri.conf.json` 全局默认透明/无边框
5. `lib.rs`：`.plugin(tauri_plugin_notification::init())`、`.plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, ...))`（Windows 传 `None`）、托盘在 `setup` 创建
6. 手动验证（Windows）：托盘出现、菜单事件、通知弹窗、自启注册表项（HKCU Run）

## ✅ 验收
- [ ] 托盘图标 + 4 个菜单项工作；通知能弹出；自启开关生效
- [ ] 窗口样式 API 按配置生效；无 Win32 直调
- [ ] `cargo clippy` 无 warning（平台代码）

## 完成动作
- 更新 `../PLAN.md` §1 状态表 → 下一步：阶段 4
- **中文 git 提交**，如：`feat: 完成平台集成（托盘/通知/自启/窗口样式）`
