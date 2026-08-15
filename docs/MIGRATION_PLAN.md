# 迁移方案（Plan）：将 `oi_sticker`（Slint 便签）的非 UI 逻辑迁移集成到 `oii_sticker`（Tauri 2）

> 版本：v1.2（全部决策已确认，可进入编码阶段）
> 生成方式：通过并行多智能体（multi-agent）对源项目 `oi_sticker` 与目标项目 `oii_sticker` 分别做了架构、业务逻辑/数据层、UI 边界与设计意图、目标工程现状四个维度的详尽分析后综合而成。
> 任务范围：**仅产出迁移方案供审查，不编写任何代码。**

---

## 0. 执行摘要（核心结论）

- **源项目 `oi_sticker`**（路径 `E:\AAACodeProjectMy\oi_sticker`，单 `i`）是一个用 **Slint 1.17 + rusqlite + tokio** 写的跨平台桌面便签应用：多窗口 Markdown 便签、三种模式（展示/交互/编辑）、SQLite 持久化、pulldown-cmark 渲染、tokio 后台提醒调度、托盘、系统通知、Win32 任务栏隐藏/置顶等。
- **目标项目 `oii_sticker`**（路径 `E:\AAACodeProjectMy\oii_sticker`，双 `i`）目前是 **Tauri 2 + Vue 3 + TypeScript 的空模板**（仅默认 `greet` 命令 + `tauri-plugin-opener`），没有任何业务逻辑。本次迁移本质是"在干净模板上落地核心库"。
- **迁移原则**：**凡是 Tauri 2 已提供的能力（窗口管理、托盘、通知、自启、透明/无边框、任务栏隐藏、点击穿透）一律改用 Tauri 原生 API / 官方插件，禁止手搓 Win32 / 原生托盘 / notify-rust 直调**；**纯业务逻辑（SQLite、Markdown 解析、时间/重复规则、slash 命令、提醒调度算法、状态缓存）整段保留并适配。**
- **不迁移的部分**：Slint 渲染层（`ui/*.slint`、`render.rs`、`RenderedLine`、`StyledText`、Slint 组件树）。UI 由 Tauri 前端（WebView）后续承载，本方案只规划其"接口面"，不构建界面。
- **最高风险**：① 许可（源为 **GPL v3**，衍生作品须同样开源）；② 手搓 Win32 与 Tauri 窗口所有权的冲突；③ 少量业务/状态机逻辑当前嵌在 Slint 内（自动滚动算法、右键双击检测、resize 三档循环），需先下沉为 Rust 纯函数；④ `remind_at` 时间字符串格式不统一；⑤ 目标工程 `capabilities` 权限需扩展。

### 已确认决策（用户反馈，2026-08-13）
- **动画功能不迁移**：提醒闪烁、脉动等所有 UI 动效均不迁移；`alert_active` 仅作为后端状态信号（通过事件下发），具体表现由新 Vue UI 自行决定，本方案不实现任何动画。
- **前端 UI 完全用 Vue 3 重新设计与渲染**：不沿用任何 Slint 组件/布局；本方案只规划后端接口面与数据契约，UI 由新 Vue 工程从零设计。
- **数据库需要迁移，新项目仍使用 SQLite**：沿用现有 schema 与 `user_version=5` 迁移机制，并支持将现有 `oi_sticker` 的 `stickers.db` 数据迁入新工程（路径切到 Tauri 的 `app_data_dir`）。
- **许可沿用 GPL v3**：`oii_sticker` 采用 **GNU General Public License v3.0**（与源一致），保留原 LICENSE 与作者声明，不换协议。
- **通知使用 `tauri-plugin-notification`**：严格采用官方插件，不保留 `notify-rust`，避免重复造轮子、规避绕过 Tauri 权限/生命周期的风险。
- **Markdown 渲染完全用 Web 技术（markdown-it）**：Rust 侧**移除** `pulldown-cmark` 的 `Block` 渲染管线（`parser.rs`/`inline.rs`/`html.rs`/`render.rs`）；前端 Vue 用 `markdown-it` 直接渲染，todo 勾选等"源行映射"由 markdown-it 的 token `map` 提供。Rust 仅保留与渲染无关的纯文本变换（`slash/` 插入、`edit_*` 编辑智能行为、`list.rs` 辅助）。

---

## 1. 分析方法与范围

| 维度 | 分析对象 | 关键产出 |
|---|---|---|
| 架构与进程生命周期 | `oi_sticker/src/main.rs`、`window.rs`、`tray.rs`、`winit_bridge.rs`、`tools/` | 启动编排、多窗口注册表、托盘、Win32 集成、tokio 调度、UI↔Rust 边界 |
| 业务逻辑与数据层 | `oi_sticker/src/{models,state,commands,db,markdown,datetime,slash,reminder}` | SQLite schema、数据模型、repo 模式、Markdown→Block 管线、重复规则语法、slash 命令、调度算法 |
| UI 结构与设计意图 | `oi_sticker/ui/*.slint`、`docs/*`、`.superpowers/sdd/*` | 组件树、逻辑/UI 拆分、三种模式、设计文档与"非目标"规划 |
| 目标工程现状 | `oii_sticker/src-tauri/**`、`src/**`、`tauri.conf.json`、`capabilities/` | 空模板确认、集成点、与 Tauri 原生能力的冲突/取舍 |

> 说明：首轮其中一个分析智能体一度误读了双 `i` 目录（把空模板当成源项目），已发现并重新在正确的单 `i` 目录执行，结论以本文为准。

---

## 2. 双项目现状核对

### 2.1 源项目 `oi_sticker`（Slint，待提取）
- 入口：`src/main.rs`（`main()` 编排启动）。
- 模块：`main.rs`、`lib.rs`、`models.rs`、`state.rs`、`commands.rs`、`window.rs`、`tray.rs`、`winit_bridge.rs`、`auto_collapse.rs`、`db/`、`reminder/`、`markdown/`、`datetime/`、`slash/`、`tools/`、`ui/`（Slint）。
- 依赖：`slint 1.17.1`、`rusqlite 0.40.1(bundled)`、`tokio 1.53.1`、`pulldown-cmark 0.13.4`、`notify-rust 4.18`、`tray-icon 0.24`、`windows-sys 0.52`、`dirs 5`、`anyhow`、`tracing`。
- build：`build.rs` 用 `slint-build` 把 `ui/*.slint` 编译为 Rust 模块（迁移时**删除**）。

### 2.2 目标项目 `oii_sticker`（Tauri 2，待填充）
- Rust 仅 `src-tauri/src/{main.rs,lib.rs}`，`lib.rs` 只有 `greet` 命令与 `tauri::Builder`。
- `Cargo.toml` 依赖：`tauri`、`tauri-plugin-opener`、`serde`、`serde_json`（无 feature 开关）。
- `capabilities/default.json`：`core:default` + `opener:default`（已含 `core:tray`、`core:window`、`core:event`、`core:path` 等默认子集）。
- `tauri.conf.json`：单窗口 800×600，**无** `decorations/transparent/alwaysOnTop`，**无** 托盘/插件配置段。
- 前端：`src/App.vue`（欢迎页）、`src/main.ts`、`package.json`（vue + `@tauri-apps/api`）。

---

## 3. 迁移目标、范围与原则

### 3.1 目标
将 `oi_sticker` 中**除 UI 渲染外**的全部功能、业务逻辑与设计思路，以"可复用的 Rust 核心库 + Tauri 命令/事件接口"的形式集成进 `oii_sticker`，使其成为一个功能等价于原便签后端、但前端可后续用 Web 技术承载的系统。

### 3.2 范围（明确边界）
- **迁移（保留并适配）**：SQLite 数据层、数据模型、`AppState` 状态缓存、时间/重复规则、slash 命令引擎（插入/匹配/状态机）、编辑智能行为（`edit_*` 纯文本变换）、提醒调度算法、配置/偏好合并、`commands` 业务函数、托盘/通知/自启/窗口样式的**设计意图**（以 Tauri API 实现）。**Markdown 渲染不迁 Rust**——交由前端 `markdown-it`（见 §9.7）。
- **丢弃（不迁移）**：`ui/*.slint`、`build.rs(slint)`、`render.rs`→`RenderedLine`、`Slint StyledText/Image`、Slint 组件树、`WindowSink` 的 Slint 闭包桥、所有 `slint::invoke_from_event_loop` 调用、**所有 UI 动画/动效（提醒闪烁、脉动等）**。
- **下沉（从 UI 抽离为纯函数后再迁移）**：自动滚动推进算法、标题栏右键双击计数、resize 三档循环（原在 `sticker_window.slint` 内）。
- **前端**（用户确认）：**完全使用 Vue 3 重新设计渲染**，不沿用任何 Slint 组件/布局/样式；本方案仅规划后端命令/事件接口与 `Block` 数据契约，UI 设计不在本 plan 的代码范围内。

### 3.3 原则
1. **Tauri 原生优先**：窗口/托盘/通知/自启/透明/任务栏隐藏/点击穿透一律用 Tauri 2 原生或官方插件（`tauri::tray`、`tauri-plugin-notification`、`tauri-plugin-autostart`、`Window` API），禁用 `tray-icon`/`notify-rust`/`windows-sys` 直调。
2. **单一 tokio runtime**：复用 Tauri 内置 `tauri::async_runtime`，**不要**再建一个 tokio runtime（避免双 runtime 冲突）。
3. **不重写纯逻辑**：`db/`、`markdown/parser`、`datetime/`、`slash/`、`commands` 纯函数段原样移植，仅改 I/O 边界。
4. **数据契约继承**：沿用现有 SQLite schema（含 `PRAGMA user_version = 5` 迁移机制），保证与旧数据库文件兼容；并提供从现有 `oi_sticker` 的 `stickers.db` 到新工程（Tauri `app_data_dir` 下）的数据迁移路径，**新项目仍使用 SQLite**。
5. **线程安全**：保留 `rusqlite` 单连接 `Arc<Mutex<Connection>>` + `spawn_blocking` 模式（Tauri 下同样适用）。

---

## 4. 文件级保留 / 丢弃 / 适配清单

> 依据：源项目各模块的"是否触碰 Slint/OS 窗口"判定。

| 源文件 | 判定 | 目标处理 |
|---|---|---|
| `src/db/schema.rs` + `schema.sql` | **保留** | 原样迁入 `src-tauri/src/db/`，保留 `user_version=5` 迁移机制 |
| `src/db/connection.rs` | **适配** | `default_db_path()` 改 `app.path().app_data_dir()`；WAL/`foreign_keys` 保留 |
| `src/db/sticker_repo.rs` | **保留** | 原样迁移（`NewSticker`/`StickerPatch` 入口结构体也带走） |
| `src/db/config_repo.rs` | **保留** | 原样迁移 |
| `src/db/prefs_repo.rs` | **保留** | 原样迁移 |
| `src/db/todo_repo.rs` | **保留（建议补）** | 补 `due_date/remind_at/remind_rule/child_sticker_id` 写入函数 |
| `src/models.rs` | **保留** | 原样迁移（`Sticker`、`StickerMode`、`StickerAttrs`、`StickerPrefs`、`EffectivePrefs`、`TodoItem`、`SystemConfig`） |
| `src/state.rs`（`AppState` 缓存 + `with_conn`） | **适配** | 保留内存缓存与 `with_conn` 调度；**删除** `WindowSink`/`SharedSink`/`windows` 映射，改用 `tauri::State` + 事件 |
| `src/commands.rs` | **适配** | 纯业务函数搬为 `#[tauri::command]`；删除 `window::*`/`sink.*` 副作用；补 `asset_repo` 替代裸 SQL |
| `src/markdown/parser.rs` + `inline.rs` + `html.rs` + `render.rs` | **丢弃** | `Block` 渲染管线，前端改用 `markdown-it`，不迁移 |
| `src/markdown/list.rs` + `edit_{indent,enter,backspace,todo}.rs` | **保留** | 纯文本变换（列表续行/缩进/删除标记/todo 行定位），供编辑器智能行为与 slash 使用，与渲染无关 |
| `src/slash/{mod,commands,matcher,insert,state}.rs` | **保留** | 原样迁移（20 条内置命令、拼音/首字母/别名匹配、插入纯函数、菜单状态机） |
| `src/datetime/{mod,parse,repeat}.rs` | **保留** | 原样迁移（自研 `DateTime`、无 chrono 依赖） |
| `src/reminder/scheduler.rs` | **适配** | 日期计算（`compute_next_remind_at`/`advance_from`/`next_occurrence`）保留；通知改 `tauri-plugin-notification`；在 `setup` 内 `tauri::async_runtime::spawn` 启动 |
| `src/slash/{mod,commands,matcher,insert,state}.rs` | **保留** | 原样迁移（20 条内置命令、拼音/首字母/别名匹配、插入纯函数、菜单状态机） |
| `src/main.rs` | **丢弃** | 被 Tauri `run()` 取代 |
| `src/lib.rs` | **丢弃/重建** | 由 Tauri `lib.rs` 重建（Builder + 命令 + setup） |
| `src/window.rs` | **丢弃（意图保留）** | 窗口生命周期意图改 Tauri `WebviewWindow` + label 注册表 |
| `src/tray.rs` | **丢弃（意图保留）** | 改 `tauri::tray::TrayIconBuilder`（菜单项/事件语义平移） |
| `src/winit_bridge.rs` | **丢弃** | 改 Tauri `Window` API（`set_skip_taskbar`/`set_always_on_top`/`start_dragging`/`set_ignore_cursor_events`） |
| `src/tools/win32.rs` + `tools/mod.rs` | **丢弃** | 自启改 `tauri-plugin-autostart`；注册表手搓删除 |
| `src/auto_collapse.rs` | **适配** | generation 计数逻辑保留；切模式动作改事件 |
| `ui/*.slint` | **丢弃** | 前端后续用 Web 承载 |
| `build.rs` | **替换** | 换成 Tauri 默认 `tauri_build::build()`（已存在） |
| `Cargo.toml`（`slint` 段） | **替换** | 换成 Tauri 2 依赖（见 §6） |

---

## 5. 目标项目模块划分

在 `oii_sticker/src-tauri/src/` 下规划如下模块（纯 Rust 核心，不依赖前端框架）：

```
src-tauri/src/
├── main.rs                 # 极薄入口，调用 lib::run()
├── lib.rs                  # Tauri Builder + 命令注册 + setup 钩子（调度器/托盘在此启动）
├── state.rs                # AppState（缓存 + with_conn 调度 + tauri::State 托管），删除 WindowSink
├── models.rs               # 全部数据模型（从源 models.rs 迁入）
├── db/
│   ├── mod.rs
│   ├── connection.rs       # 连接 + 路径改为 app_data_dir
│   ├── schema.rs           # schema.sql + 迁移（user_version=5）
│   ├── sticker_repo.rs
│   ├── config_repo.rs
│   ├── prefs_repo.rs
│   ├── todo_repo.rs        # 补提醒字段写入
│   └── asset_repo.rs       # 新增：补源项目缺失的 assets repo
├── markdown/
│   ├── mod.rs
│   ├── list.rs              # 有序/嵌套列表续行辅助（edit_enter 依赖）
│   └── edit_{indent,enter,backspace,todo}.rs   # 编辑器智能行为（纯文本变换，保留）
├── datetime/
│   ├── mod.rs / parse.rs / repeat.rs
├── slash/
│   ├── mod.rs / commands.rs / matcher.rs / insert.rs / state.rs
├── reminder/
│   ├── mod.rs
│   └── scheduler.rs        # 纯算法 + 通知（改 Tauri 通知）
├── commands.rs             # 业务命令（create/update/delete/set_reminder/set_config/toggle_todo/slash…）
├── platform/               # 平台集成（Tauri 原生实现源项目的 Win32/托盘/通知意图）
│   ├── tray.rs             # TrayIconBuilder + 菜单/事件
│   ├── notify.rs           # tauri-plugin-notification 封装
│   ├── autostart.rs        # tauri-plugin-autostart 封装
│   └── window_style.rs     # 透明/无边框/置顶/任务栏隐藏/点击穿透/拖动 的 Tauri API 封装
└── events.rs               # 事件名常量 + emit 封装（替代 WindowSink 闭包）
```

前端（`src/`）仅规划"接口面"（本方案不构建 UI）：后续新增 `src/stores/`、`src/composables/useTauri.ts`、`src/components/Note*.vue`，通过 `invoke` 调命令、通过 `listen` 收事件。

---

## 6. 依赖处理

### 6.1 删除（随 Slint 一起移除）
- `slint`、`i-slint-backend-winit`、`slint-build`（含 `build.rs` 的 slint 编译）。
- `tray-icon`（改用 `tauri::tray`）、`notify-rust`（改用 `tauri-plugin-notification`）、`windows-sys`（改用 Tauri `Window` API）、`once_cell`（源未真正使用）。

### 6.2 保留
- `rusqlite = { version = "0.40", features = ["bundled"] }`
- `anyhow = "1"`、`tracing` + `tracing-subscriber`
- `serde` + `serde_json`（目标已含）
- *注：`pulldown-cmark` **不再保留**——Markdown 渲染改由前端 `markdown-it` 承担（见 §9.7 / §6.4）。*

### 6.3 替换 / 新增（Tauri 侧 / 前端侧）
- `tauri = { version = "2", features = [...] }`：按需开启 `tray-icon` feature（Tauri 托盘后端所需）、`image-png` 等。
- `tauri-plugin-opener`（已含）
- `tauri-plugin-notification = "2"`（替代 notify-rust，用户确认采用官方插件）
- `tauri-plugin-autostart = "2"`（替代注册表手搓）
- **不要**单独引入 `tokio`：直接用 Tauri re-export 的 `tauri::async_runtime` / `tauri::async_runtime::spawn_blocking`。若某些源代码显式 `use tokio`，可保留 `tokio` 作为依赖以最小改动适配（但禁用其独立 runtime 构建）。
- `dirs` 可删（改用 `app.path()`），或保留作兼容。
- **前端新增依赖**：`markdown-it`（+ `@types/markdown-it` 或 `markdown-it` 的 TS 类型）用于 Markdown 渲染；`rusqlite`/`pulldown-cmark` 不出现在前端。

### 6.4 依赖对照表

| 源依赖 | 目标处理 |
|---|---|
| slint / i-slint-backend-winit / slint-build | 删除 |
| rusqlite (bundled) | 保留 |
| pulldown-cmark | **移除**（前端 `markdown-it` 替代，Rust 不再做 Markdown 渲染） |
| tokio | 改用 `tauri::async_runtime`（或保留依赖但复用 Tauri runtime） |
| notify-rust | → `tauri-plugin-notification` |
| tray-icon | → `tauri::tray` |
| windows-sys | → Tauri `Window` API（Win32 手搓删除） |
| dirs | → `app.path().app_data_dir()`（或保留） |
| anyhow / tracing | 保留 |
| once_cell | 删除（源未用） |

---

## 7. 文件结构规划

### 7.1 Rust 侧（`src-tauri/`）
见 §5。要点：
- `lib.rs` 注册命令：`generate_handler![greet(删除), create_sticker, delete_sticker, update_sticker, set_reminder, clear_reminder, acknowledge_alert, set_config, update_sticker_prefs, reset_sticker_prefs, toggle_todo, import_asset, list_stickers, get_sticker, ...]`。
- `lib.rs::run()` 用 `.manage(AppState::new(...))` 托管状态；`.setup(|app| { 启动 scheduler; 安装托盘; })`。
- `tauri.conf.json` 增加窗口 `decorations:false`、`transparent:true`、`alwaysOnTop`、`resizable`，并新增"便签窗口模板"（或运行时 `WebviewWindowBuilder` 动态创建，按 `sticker-<id>` label 寻址）。

### 7.2 前端侧（`src/`，仅接口面，不构建 UI）
- `invoke("list_stickers")` → 便签列表
- `invoke("create_sticker", {...})` / `invoke("update_sticker", {id, patch})`
- `invoke("set_reminder", {id, attrs})`
- `listen("sticky://alert-active", ...)` / `listen("sticky://push-update", ...)` 接收后端推送（替代 `WindowSink`）
- **Markdown 渲染**：Vue 组件内用 `markdown-it` 将便签 `content` 渲染为 HTML（任务清单用 `markdown-it-task-lists` 或自写 rule）；todo 勾选通过 token `map` 取得源行号 → `invoke("toggle_todo", {id, line})`。

---

## 8. 接口适配（核心改造点）

### 8.1 状态管理：从 `thread_local`/闭包桥 → `tauri::State` + 事件
- 源：`AppState` 持 `inner: Arc<RwLock<AppStateInner>>`、`conn: Arc<Mutex<Connection>>`、`windows: Arc<Mutex<HashMap<i64, SharedSink>>>`；UI 推送经 `WindowSink` 闭包 + `invoke_from_event_loop`。
- 目标：用 `app.manage(state)` 托管 `AppState`；**删除 `WindowSink`/`windows` 映射**；后端→前端的推送改为 `app.emit_to(label, "sticky://push-update", payload)`（封装于 `events.rs`）。前端按便签 `label`（如 `sticker-<id>`）监听。

### 8.2 多窗口：从 `thread_local Vec<StickerWindow>` → label 注册表
- 源：`STICKER_WINDOWS: thread_local RefCell<Vec<StickerWindow>>`，因 Slint 组件 `!Send`。
- 目标：Tauri `WebviewWindow` 可跨线程寻址，用 `app.get_webview_window("sticker-<id>")` 或在 `AppState` 内维护 `HashMap<String, Weak<WebviewWindow>>`（或直接每次按 label 查询）替代 `HashMap<i64, SharedSink>`。窗口几何/置顶/透明用 `Window` API。

### 8.3 命令边界：Slint `on_*` 回调 → `#[tauri::command]`
- 源 `window.rs` 中每个 `on_*` 回调即一个 Rust 入口（如 `on_create_sticker`、`on_save_clicked`、`on_toggle_todo`、`on_resize_window`、`on_opacity_changed`…）。逐个映射为 Tauri 命令，参数用 `serde` 结构体；前端 `invoke` 调用。

### 8.4 反向推送：调度器/自动收起 → 事件而非直接窗口调用
- 源：scheduler/auto_collapse 在 tokio 线程调 `sink.set_alert_active(true)`（内部 `invoke_from_event_loop`）。
- 目标：`app.emit_to(format!("sticker-{}", id), "sticky://alert-active", true)`。前端据事件触发闪烁/切模式。

### 8.5 线程模型：去掉 `rt.enter()` + `block_on`
- 源：`main.rs` 建独立 tokio runtime 并 `rt.enter()` 使主线程可 `block_on` DB。
- 目标：Tauri 自带 runtime；命令可直接 `async` + `await`（`with_conn` 内部用 `tauri::async_runtime::spawn_blocking`）。scheduler 用 `tauri::async_runtime::spawn`。**切勿再建第二个 runtime**。

### 8.6 数据库路径
- 源：`dirs::data_dir()/OiSticker/stickers.db`。
- 目标：`app.path().app_data_dir()` 下 `stickers.db`（符合 Tauri 沙箱/打包路径规范）。

### 8.7 平台能力（用 Tauri 原生 API 替换手搓）
| 源（手搓） | 目标（Tauri 原生） |
|---|---|
| `winit set_skip_taskbar` hook | `Window::set_skip_taskbar(true)` |
| `w.set_cursor_hittest(false)` 点击穿透 | `Window::set_ignore_cursor_events(true)` |
| `w.set_window_level(AlwaysOnTop)` | `Window::set_always_on_top(true)` |
| `w.drag_window()` | `Window::start_dragging()` / 前端 `window.startDragging()` |
| `tray-icon` 自建托盘 | `tauri::tray::TrayIconBuilder` + `on_menu_event` + `on_tray_icon_event` |
| `notify-rust` 直调 | `tauri-plugin-notification` |
| `tools/win32.rs` 注册表 Run 键 | `tauri-plugin-autostart` |
| `no-frame/transparent/always-on-top`（Slint 属性） | `tauri.conf.json` 窗口 `decorations:false`+`transparent:true`+`alwaysOnTop:true` |

---

## 9. 关键技术实现要点

1. **SQLite 并发**：保留单连接 `Arc<Mutex<Connection>>`；所有 repo 调用经 `state.with_conn(|c| ...)` → `spawn_blocking`。迁移机制（每步事务、`user_version=5`、幂等 `INSERT OR IGNORE`/`IF NOT EXISTS`）原样保留。**注意补 `asset_repo` 与 `completion_log` 写入**（源项目 assets 无 repo、completion_log 为死表）。
2. **提醒调度器**：`setup` 内 `tauri::async_runtime::spawn` 一个 10s `interval` 循环（可沿用 `time::interval`，但用 Tauri runtime）；命中后下发 `alert_active` 状态事件（**仅状态信号，不含任何动画/闪烁**，由新 Vue UI 自行表现）+ 调 `tauri-plugin-notification` 弹通知；`compute_next_remind_at`/`advance_from`/`next_occurrence` 纯逻辑复用。
3. **托盘**：`TrayIconBuilder` 重建菜单（新建便签/打开主控台/系统设置/退出），菜单事件映射到对应命令/事件。**图标**：源实际是运行时生成的纯绿 RGBA（并非 PNG 内嵌），迁移时直接用 `tauri::image::Image::from_rgba` 或放置一张正式 PNG 资源。
4. **通知**：封装 `platform/notify.rs`，调用 `tauri-plugin-notification`；保留原 payload（标题/正文/`appname: Oi Sticker`）。
5. **自启**：封装 `platform/autostart.rs`，调用 `tauri-plugin-autostart`（`is_enabled`/`enable`/`disable`），替代注册表手搓；保持"非 Windows 空操作"语义。
6. **窗口样式**：在 `tauri.conf.json` 设置默认透明/无边框/置顶；运行时按便签 `display_mode`/偏好调整 `set_skip_taskbar`/`set_always_on_top`/`set_ignore_cursor_events`。
7. **Markdown（渲染移到前端）**：Rust 侧**不再**做 Markdown→`Block` 渲染，故 `parser.rs`/`inline.rs`/`html.rs`/`render.rs` 整体不迁移；仅保留与渲染无关的纯文本变换（`list.rs` + `edit_*`）。**前端 Vue 用 `markdown-it` 渲染**便签内容为 HTML：
   - 标题/粗体/斜体/删除线/列表/任务清单/引用/链接/表格/图片等由 `markdown-it` 标准插件覆盖（任务清单需 `markdown-it-task-lists` 或自写 rule）。
   - **todo 勾选源行映射**：`markdown-it` 的 token 带 `token.map = [startLine, endLine]`（0-based 源行），前端据此把"勾选第 N 个 todo"映射回 Markdown 源行，调用 `toggle_todo` 命令（参数即源行号）——等价于源 `Block::Todo.line` 的作用。
   - **图片**：`asset://` 或本地路径经前端解析为 `<img>`（源 `asset://` 协议属"非目标"，本次可用普通本地文件 URL 或 `<img src="file://...">` 替代，需 Tauri `core:permission`/`asset` 协议授权）。
   - 保留源项目对 CJK 排版（`insert_wrap_hints` 每 N 字插零宽空格）的意图，可在前端 CSS `word-break`/`overflow-wrap` 实现，无需 Rust。
8. **slash 引擎**：`slash/*` 纯逻辑整段保留；前端负责浮层 UI 与键盘导航（源里导航在 Slint `key-pressed`，属 UI 职责）。
9. **状态机下沉**：把 `sticker_window.slint` 内的自动滚动推进、`右键双击计数`、`resize 三档循环` 抽成 `markdown`/新增 `ui_logic` 模块的 Rust 纯函数，再被 Tauri 命令/前端调用——保证"核心可复用"。

---

## 10. 分阶段实施路线图（仅步骤，不编码）

- **阶段 0 — 准备**：在 `oii_sticker` 引入 `tauri-plugin-notification`/`tauri-plugin-autostart`；扩展 `capabilities/default.json`（加 `notification:default`、`autostart:default`）；配置 `tauri.conf.json` 窗口样式；清理 `Cargo.toml` 删除 Slint 依赖、加 rusqlite/pulldown-cmark。
- **阶段 1 — 数据层**：迁入 `db/`（schema/connection/models/repos），`connection` 改 `app_data_dir`；用 Tauri `setup` 内 `app.manage(AppState)`。验证数据库创建、迁移、CRUD。
- **阶段 2 — 纯逻辑**：迁入 `markdown/`（仅 `list.rs` + `edit_*`）、`datetime/`、`slash/`（不依赖任何 UI 框架）。*Markdown 渲染不在此步，由前端 `markdown-it` 承担。*
- **阶段 3 — 状态与命令**：迁入 `state.rs`（`AppState` + `with_conn`，删 `WindowSink`）；把 `commands.rs` 纯业务函数改写为 `#[tauri::command]`；建立 `events.rs`。
- **阶段 4 — 提醒调度**：迁入 `reminder/scheduler.rs` 算法，在 `setup` 内 `spawn`；通知改 Tauri 插件；emit 闪烁事件。
- **阶段 5 — 平台集成**：`platform/tray.rs`、`notify.rs`、`autostart.rs`、`window_style.rs` 用 Tauri 原生 API 重建源项目意图。
- **阶段 6 — 事件桥与前端接口面**：定义事件名常量；在 `App.vue`/新组件里 `listen` 接收 `push-update`/`alert-active`；保证 `invoke` 契约与源 `on_*` 回调一一对应（先跑通"创建/列表/编辑/提醒/托盘新建"主链路）。
- **阶段 7 — 测试与对等性**：跑源项目既有测试（`schema` 迁移回滚测试、`markdown` 解析测试、`datetime` 测试、`slash` 测试）；补齐 `todo_repo` 提醒字段、`asset_repo`；核对三种模式语义与旧数据库兼容。

---

## 11. 潜在风险与缓解

| # | 风险 | 严重度 | 缓解 |
|---|---|---|---|
| **R1** | **许可（GPL v3）**：源为 GPL v3，衍生物须同样以 GPL v3 开源。 | **已确认（低）** | 用户已确认沿用 **GPL v3**；保留原 LICENSE 与作者声明，不换协议。迁移无法律障碍。 |
| **R2** | **手搓 Win32 与 Tauri 冲突**：源直接操作窗口 HWND/样式，会与 Tauri 窗口所有权冲突导致崩溃或失效。 | 高 | 严格使用 Tauri `Window` API 与官方插件，删除 `winit_bridge.rs`/`tools/win32.rs` 的 Win32 直调。 |
| **R3** | **双 tokio runtime**：源自建 runtime，Tauri 也有，并发会冲突。 | 高 | 删除源 runtime 构建，统一用 `tauri::async_runtime`。 |
| **R4** | **逻辑嵌在 Slint 内**：自动滚动算法、右键双击检测、resize 三档循环当前在 `.slint`，迁移会丢失或重复实现。 | 中 | 先下沉为 Rust 纯函数（阶段 9.9），再迁移；否则这些交互在 Tauri 下需重写。 |
| **R5** | **`remind_at` 格式不统一**：混用 `YYYY-MM-DD HH:MM` 与 `YYYY-MM-DDTHH:MM:SS`。 | 中 | 迁移前统一为一种格式（建议 ISO `YYYY-MM-DDTHH:MM:SS`），做数据迁移/解析兼容。 |
| **R6** | **`assets` 无 repo、`completion_log` 死表**：源项目数据层不完整。 | 低 | 补 `asset_repo`；`completion_log` 暂不实现或按设计文档决定是否落地"已完成任务聚合"（属源非目标）。 |
| **R7** | **E 键进编辑未实现**：设计承诺但 Slint 中无 `Key.E` 处理。 | 低 | 迁移时在前端实现或明确标注为"待办"。 |
| **R8** | **display 模式语义分歧**：设计文档曾写"真实点击穿透"，已变更为"低透明度收起"，且用户确认**动画不迁移**（无闪烁/脉动）。 | 低 | 维持低 alpha 方案（源已决策），`alert_active` 仅作状态信号经事件下发；新 Vue UI 据 `display_mode` 渲染，不实现动画。 |
| **R9** | **跨线程直接操作窗口**：源 scheduler/auto_collapse 曾直接调窗口方法（P0 已知问题），需包 `invoke_from_event_loop`。 | 中 | Tauri 下改为 `emit_to` 事件，前端处理 UI 变更；绝不在 async 线程直接触碰 Webview。 |
| **R10** | **capabilities 权限不足**：托盘/通知/自启/路径访问需扩展权限。 | 中 | 在 `capabilities/default.json` 增加 `notification:default`、`autostart:default`、`core:tray:allow-*`（如需要）等。 |
| **R11** | **状态缓存并发**：`AppState` 用 `RwLock<Inner>` + `Mutex<Connection>`，多窗口并发写入需验证。 | 中 | 沿用源的锁策略；写操作走 `spawn_blocking` 单连接，避免死锁；`with_conn` 复用。 |
| **R12** | **旧数据库兼容**：若用户已有 `oi_sticker` 的 `.db`，目标须能读取（`user_version=5`）。 | 低 | 保留 schema 迁移机制与生产库同路径或迁移工具。 |
| **R13** | **Slint `effective-*` 默认值合并语义**：`EffectivePrefs`/`SystemConfig::effective` 的优先级链（prefs→sticker→system→兜底）。 | 低 | 整段保留 `models.rs` 的合并逻辑，勿在前端重新实现以免不一致。 |
| **R14** | **Markdown 渲染差异（pulldown-cmark→Block 自绘 vs markdown-it）**：源用自研 `Block` 管线（含复合编号行预处理、行内 markdown 回写、`Block::Todo.line` 源行），前端改用 `markdown-it` 后表现需对齐。 | 中 | 前端用 `markdown-it` + task-lists 插件；todo 勾选改由 `token.map` 源行映射实现；CJK 排版用 CSS；表格/图片/引用确保等价。上线前做"同一份 Markdown 源"的渲染对照测试。 |

---

## 12. 验收标准（Done 定义）

- [ ] `oii_sticker` 启动后自动建库并跑通 `user_version=5` 迁移；已有旧库可兼容打开。
- [ ] 通过 Tauri 命令可完成便签的增/删/改/查、偏好合并。
- [ ] 前端 `markdown-it` 正确渲染 Markdown（标题/列表/任务清单/引用/链接/表格/图片），todo 勾选可映射回源行并调用 `toggle_todo`。
- [ ] 托盘菜单（新建/打开主控台/设置/退出）工作；系统通知在提醒触发时弹出。
- [ ] 提醒调度器按 `daily/weekly/interval:N/monthly:N/yearly:M-D` 正确续期并追补关机期间错过的周期。
- [ ] slash 命令引擎（20 条 + 匹配）可用；自动收起/模式切换语义与原版一致（display=低透明收起，**无动画**）。
- [ ] 窗口透明/无边框/置顶/任务栏隐藏/（可选）点击穿透按配置生效。
- [ ] 前端 Vue 3 UI 按新设计实现并对接命令/事件（不沿用 Slint 布局）。
- [ ] 旧 `oi_sticker` 的 `stickers.db` 可迁移至新工程（路径为 Tauri `app_data_dir`），数据不丢失。
- [ ] 源项目既有单测（schema/markdown/datetime/slash）在 Tauri 工程下仍全绿。
- [ ] 不含任何 `slint`、`tray-icon`、`notify-rust`、`windows-sys` 直调；不建第二个 tokio runtime。
- [ ] 许可合规（GPL v3 声明保留）。

---

## 13. 决策状态总览（全部已确认 ✅）

| # | 决策点 | 结论 |
|---|---|---|
| 1 | 许可 | **GPL v3**（与源一致，保留 LICENSE 与作者声明） |
| 2 | 前端形态 | **完全用 Vue 3 重新设计渲染**，不沿用 Slint 组件/布局 |
| 3 | 通知实现 | **`tauri-plugin-notification`**（官方插件，不保留 notify-rust） |
| 4 | display 模式 / 动画 | 低透明收起；**动画不迁移**（alert_active 仅状态信号） |
| 5 | 数据库 | **迁移且仍用 SQLite**（沿用 schema + user_version=5，旧 `stickers.db` 可迁入） |
| 6 | 非目标功能 | 动画/脉动等动效不迁移；多级待办、已完成聚合、`asset://` 等按源"非目标"暂不纳入 |
| 7 | Markdown 渲染 | **前端 `markdown-it`**（Rust 移除 pulldown-cmark 的 Block 管线） |

> **所有阻塞项已关闭，可进入"Plan 批准后编码"阶段**，按 §10 阶段 0→7 落地。遗留的可选细化（如 `completion_log`/`asset_repo` 是否补齐、图片资源协议方案）可在实施中按优先级决定，不阻塞主线。

---

> 本方案为审查草稿（v1.2，全部决策已确认），未包含任何代码实现。所有阻塞项已关闭，可进入"Plan 批准后编码"阶段，按 §10 阶段 0→7 顺序落地。
