# 重新开发方案（Greenfield）：在 Tauri 2 中全新实现 oi_sticker 功能

> 版本：v1.0（全新开发 plan，取代 `MIGRATION_PLAN.md`）
> 生成方式：基于此前对 `oi_sticker`（Slint）的详尽分析（架构/数据层/UI 边界/设计意图），将其**功能与设计意图**作为需求规格，但**技术栈与代码 100% 重新开发**，不复用任何 Slint 代码。
> 范围：**仅产出开发方案供审查，不编写实现代码。**

---

## 0. 方案变更说明（为何重写而非移植）

经前期对 `oi_sticker`（Slint 1.17）的源码分析，发现若做"非 UI 逻辑移植"需面对以下高成本改造：
- 业务/状态机逻辑嵌在 `.slint` 内（自动滚动、右键双击、resize 三档循环）需先下沉为 Rust 纯函数；
- 手搓 Win32（`winit_bridge`/`tools/win32`）与 Tauri 2 的窗口所有权冲突，必须改用 Tauri API 重建；
- 需拆除 `WindowSink` 闭包桥、自建 tokio runtime，改为 `tauri::State` + 事件；
- `pulldown-cmark` 的 `Block` 渲染管线在 Tauri 下本就要废弃（前端用 `markdown-it`）。

**结论**：直接以 Tauri 2 + Vue 3 从零重写，以 `oi_sticker` 的功能为"需求规格"，更简洁、更快速，且无历史包袱。本方案即据此重做。

---

## 1. 目标与范围

### 1.1 目标
全新开发一个跨平台桌面便签应用（工作名沿用 `oii_sticker`），**功能仿照** `oi_sticker`：多窗口 Markdown 便签、三种模式、SQLite 持久化、提醒调度、托盘、通知、自启、窗口样式个性化、slash 命令。

### 1.2 范围
- **实现（功能对标）**：见 §3 功能规格。
- **不实现（已确认的非目标）**：所有 UI 动画/动效（提醒闪烁、脉动等）；多级待办便签、已完成任务聚合、`asset://` 图片协议等源项目"非目标"功能（本次不纳入）。
- **技术全新**：Tauri 2 + Vue 3 + TS；Rust 核心 + rusqlite + 官方 Tauri 插件；前端 `markdown-it` 渲染。**不再出现** `slint` / `pulldown-cmark` / `tray-icon` / `notify-rust` / `windows-sys`。

### 1.3 已确认决策（全部沿用）
| 决策 | 结论 |
|---|---|
| 许可 | **GPL v3**（保留 LICENSE 与作者声明） |
| 前端 | **完全用 Vue 3 重新设计渲染** |
| 通知 | **`tauri-plugin-notification`**（官方插件） |
| 动画 | **不迁移/不实现**（`alert_active` 仅状态信号） |
| 数据库 | **使用 SQLite**（沿用既有 schema 设计以保证旧数据可接入） |
| Markdown | **前端 `markdown-it` 渲染**（Rust 不做 Markdown 渲染） |

---

## 2. 功能规格（来自 oi_sticker 分析，作为需求）

> 以下即"要重新实现什么"。分组对标源项目 README / `docs/design.md` / 代码分析结论。

### 2.1 便签管理
- 多便签并存：桌面同时多个独立便签窗口。
- 创建 / 销毁：托盘或主控台新建；关闭即销毁窗口（保留数据）。
- 拖动 / 缩放：编辑模式下自由拖动位置、调整大小。
- 位置与状态持久化：坐标、尺寸、模式、透明度等自动存库，重启恢复。
- （可选/非目标）子便签：标题（h1–h6）转子便签——本期不实现。

### 2.2 Markdown 渲染（`markdown-it`）
- 基础：标题、**粗体**、*斜体*、~~删除线~~、行内代码、分割线。
- 列表：有序 / 无序 / 嵌套；**任务清单** `- [ ]` / `- [x]`（交互模式可勾选）。
- 引用、链接（点击跳转）、表格、图片。
- todo 勾选需映射回 Markdown **源行号**（由 `markdown-it` 的 `token.map` 提供）。

### 2.3 提醒与待办
- `remind_at` 到点触发系统通知 + `alert_active` 状态信号（无动画）。
- 重复规则（全新实现，语法沿用源设计）：`daily` / `weekly[:mon,wed]` / `interval:N` / `monthly:N` / `yearly:M-D`。
- 后台调度：tokio 定时器（用 Tauri runtime）周期扫描到期项，续期并追补关机期间错过的周期（上限 366 次）。

### 2.4 个性化偏好
- 背景色（十六进制）、窗口透明度（0~1）、置顶、自动滚动（速度可调）、标题/正文字号、标题居中、文本颜色。
- 系统默认值：未单独设置时沿用 `system_config` 全局默认；合并优先级 `prefs → sticker → system → 兜底`（保留源 `EffectivePrefs` 语义）。

### 2.5 系统集成
- 任务栏隐藏：便签窗口不出现在任务栏 / Alt+Tab（Tauri `set_skip_taskbar`）。
- 系统托盘：托盘图标 + 菜单（新建便签 / 打开主控台 / 系统设置 / 退出）。
- 系统通知：`tauri-plugin-notification`。
- 斜杠命令：编辑模式 `/` 触发命令菜单（标题/列表/代码/引用/链接/图片/表格/折叠框等约 20 条），支持中文/拼音/首字母/别名检索。
- 鼠标穿透（可选）：展示模式可设置点击穿透（Tauri `set_ignore_cursor_events`），本期可选实现。

### 2.6 三种模式
- **display（展示）**：只读、低透明度收起、仅响应右键双击唤醒。
- **interact（交互）**：可点 todo、展开/折叠；5 秒无操作自动收起回 display（编辑态不被收起）。
- **edit（编辑）**：可拖动/缩放/编辑 Markdown/设属性；含斜杠命令、智能缩进等编辑器行为。

---

## 3. 技术栈与依赖

### 3.1 Rust（`src-tauri`）
- `tauri = { version = "2", features = ["tray-icon", "image-png"] }`
- `tauri-plugin-opener`（已含）、`tauri-plugin-notification`、`tauri-plugin-autostart`
- `rusqlite = { version = "0.40", features = ["bundled"] }`
- `serde` + `serde_json`（已含）、`anyhow`、`tracing` + `tracing-subscriber`
- **不引入** `tokio` 依赖（直接复用 Tauri re-export 的 `tauri::async_runtime`）；若个别处需 `spawn_blocking` 用 `tauri::async_runtime::spawn_blocking`。

### 3.2 前端（`src`）
基础：`vue` 3 + `vite` + `typescript`。
Tauri 桥接：`@tauri-apps/api`、`@tauri-apps/plugin-notification`、`@tauri-apps/plugin-autostart`。
渲染：`markdown-it`（+ `markdown-it-task-lists` 或自写 todo rule）。
**常用库（按你的需求纳入）**：
- `pinia` —— **状态管理（必选）**：便签列表 / 偏好 / 设置的单一真相源 store；命令返回即更新，事件驱动跨窗口同步。
- `vue-router` —— 应用内视图路由（主控台内部的多页签、设置页、关于页等；Tauri 多窗口负责"主控台 vs 便签窗口"的 OS 级切换，router 负责窗口内的视图切换）。
- `@vueuse/core` —— 组合式工具：`useStorage`（持久化前端偏好）、`useDraggable`/`useEventListener`/`onClickOutside`/`useDark` 等，减少手写 DOM/事件逻辑，助力窗口拖拽与全局事件。
- `vuedraggable`（Vue3 用 `vuedraggable@next` / `vue-draggable-plus`）—— 便签列表拖拽排序、slash 菜单项排序等可排序列表。
- `dayjs` —— 前端日期/时间解析与格式化（用于时间选择器的展示与解析；Rust 侧仍负责重复规则计算，前后端格式需统一）。
- `sass`（dart-sass，devDependency）—— CSS 预处理器，用于样式变量/嵌套/mixin 组织（Less 亦可，二选一，建议 sass）。
- （可选）`@iconify/vue` 或 `unplugin-icons` —— 图标管理，避免散落 SVG。
**不引入**：`pulldown-cmark` 相关任何东西（渲染已由 `markdown-it` 承担）。

---

## 4. 目标项目模块结构（全新）

### 4.1 Rust 核心（`src-tauri/src/`）
```
src-tauri/src/
├── main.rs                 # 极薄入口，调用 lib::run()
├── lib.rs                  # Tauri Builder + 命令注册 + setup（调度器/托盘/自启启动）
├── state.rs                # AppState（内存缓存 + with_conn 调度），用 tauri::State 托管
├── models.rs               # Sticker / StickerMode / StickerAttrs / StickerPrefs / EffectivePrefs / TodoItem / SystemConfig
├── db/
│   ├── mod.rs
│   ├── connection.rs       # 连接 + 路径用 app.path().app_data_dir()
│   ├── schema.rs           # schema.sql + 迁移（PRAGMA user_version，事务化、幂等）
│   ├── sticker_repo.rs
│   ├── config_repo.rs
│   ├── prefs_repo.rs
│   ├── todo_repo.rs
│   └── asset_repo.rs       # 图片资源（可选，按 §2.5 图片需求）
├── datetime/               # 全新实现：DateTime、parse、repeat（daily/weekly/interval/monthly/yearly）
├── reminder/
│   └── scheduler.rs        # 用 tauri::async_runtime 启动的定时扫描 + 通知 + 续期
├── slash/                  # 全新实现：commands / matcher（拼音+首字母+别名）/ insert / state
├── editing/                # 编辑器纯文本变换：indent / enter / backspace / todo（行定位）
├── commands.rs             # 业务命令（create/update/delete/set_reminder/set_config/toggle_todo/slash…）
├── platform/
│   ├── tray.rs             # TrayIconBuilder + 菜单/事件
│   ├── notify.rs           # tauri-plugin-notification 封装
│   ├── autostart.rs        # tauri-plugin-autostart 封装
│   └── window_style.rs     # 透明/无边框/置顶/任务栏隐藏/点击穿透/拖动 的 Tauri API 封装
└── events.rs               # 事件名常量 + emit 封装（替代旧 WindowSink）
```

### 4.2 前端（`src/`）
```
src/
├── main.ts
├── App.vue                 # 极薄入口：按窗口 label 决定挂载 Console 还是 StickerWindow；内含 <router-view>
├── router/
│   └── index.ts           # 窗口内视图路由（设置页/关于页/主控台子页等）
├── styles/                # sass 全局样式与变量（_variables.scss / _mixins.scss / global.scss）
├── stores/                # pinia：notes.ts / settings.ts / prefs.ts
├── composables/
│   ├── useTauri.ts         # invoke / listen 封装
│   ├── useSticker.ts       # 单便签状态与命令（v-model 绑定源）
│   ├── useDraggable.ts     # 基于 @vueuse/core 的窗口拖动/缩放封装
│   └── useReminder.ts       # 提醒时间格式化（dayjs）
├── components/
│   ├── console/            # 主控台相关（按功能拆分，不堆在一个文件）
│   │   ├── ConsoleList.vue       # 便签列表
│   │   ├── ConsoleListItem.vue   # 单行（标题/预览/提醒/显示-隐藏-删除）
│   │   ├── ConsoleToolbar.vue    # 刷新/新建/退出
│   │   └── SettingsPanel.vue     # 系统设置弹层（内含通用/便签两个子页）
│   ├── sticker/            # 单便签窗口相关（强烈拆分）
│   │   ├── StickerWindow.vue     # 容器：根据 mode 切换子视图，不写全部逻辑
│   │   ├── StickerHeader.vue     # 标题栏（拖动/居中/E/⚙/×/双击唤醒）
│   │   ├── StickerViewer.vue     # interact 模式：Markdown 渲染 + todo 勾选 + 折叠
│   │   ├── StickerEditor.vue     # edit 模式：标题/截止/提醒/正文编辑 + 斜杠菜单
│   │   ├── StickerSettings.vue   # 偏好设置弹层
│   │   ├── StickerResizeHandle.vue # 右下角缩放把手
│   │   └── ReminderPicker.vue    # 提醒时间 + 重复规则选择
│   ├── markdown/
│   │   ├── MarkdownView.vue      # markdown-it 渲染封装（todo 源行映射）
│   │   └── TaskItem.vue          # 单个任务清单项（勾选 emit 源行）
│   ├── slash/
│   │   ├── SlashMenu.vue         # 斜杠命令浮层
│   │   └── SlashMenuItem.vue     # 单项
│   ├── common/             # 通用小组件（Button / Modal / ColorInput / Slider / Stepper 等）
│   └── DateTimePicker.vue  # 时间选择（dayjs 格式化）
├── utils/                 # 纯函数：markdown 源行映射、重复规则展示文案等
└── types.ts                # 前后端共享数据结构（Sticker / StickerAttrs / …）
```
**Vue 文件拆分原则**：每个 `.vue` 只负责单一职责，**单文件控制在 ~300–400 行以内**；超过即继续拆子组件（如 `StickerWindow` 仅做模式分发，viewer/editor/settings 各自独立）。严禁把整个便签窗口的 HTML/逻辑堆在一个文件。

### 4.3 工程结构原则（Rust + 前端）
- **Rust 侧**：大功能块 = 独立文件夹（`db/` `datetime/` `reminder/` `slash/` `editing/` `platform/`）；子功能块 = 独立 `.rs` 文件（如 `platform/` 下分 `tray.rs` / `notify.rs` / `autostart.rs` / `window_style.rs`；`db/` 下分各 `*_repo.rs`）。禁止把多个不相关功能塞进一个 `.rs`。
- **前端侧**：大功能块 = 独立文件夹（`components/console`、`components/sticker`、`components/markdown`、`stores`、`composables`、`styles`）；子功能 = 独立组件/文件；同上"单文件不过大"原则。
- **双向绑定（v-model）贯穿全局**：设置表单、便签编辑表单、偏好、时间选择器、slash 查询等一律用 `v-model` / `defineModel()`（Vue 3.4+）做双向绑定；子组件通过 `defineModel` 暴露可写属性，父组件以 `v-model` 接驳；pinia store 作为单一真相源，组件绑定 store 字段（必要时经 `computed` get/set 桥接命令调用）。避免大量手动 `emit` + 单向 `prop` 散弹式更新。

### 4.4 关键架构决策：多窗口模型
**采用 Tauri 多窗口**（每个便签 = 一个真实 `WebviewWindow`），以忠实还原"每便签独立 OS 窗口、可单独置顶/透明/隐藏任务栏/拖拽"的特性：
- 主控台窗口 label = `main`；便签窗口 label = `sticker-<id>`。
- 前端 `App.vue` 读取 `getCurrentWindow().label()` 决定加载 `Console` 还是 `StickerWindow`。
- 新建便签：`commands.create_sticker` 写库后，Rust 侧 `WebviewWindowBuilder` 创建 `sticker-<id>` 窗口并应用样式。
- 备选（不推荐）：单窗口 + DOM 浮层便签——更简单但丧失 OS 级每窗口置顶/透明/任务栏控制，与 oi_sticker 体验不符，故不采用。

---

## 5. 数据模型与 SQLite Schema（复用设计，全新实现）

沿用经分析确认的 schema 设计（保证与 `oi_sticker` 旧 `stickers.db` 可兼容接入），但**代码全新编写**：

| 表 | 关键列 | 说明 |
|---|---|---|
| `stickers` | id, parent_id, title, content, heading_level, pos_x/y, width/height, opacity, bg_color, always_on_top, auto_scroll, is_completed, alert_active, display_mode, created_at, updated_at | 便签主表 |
| `sticker_attrs` | sticker_id(PK), due_date, remind_at, remind_rule, is_recurring | 提醒属性 1:1 |
| `todo_items` | id, sticker_id, child_sticker_id, text, done, completed_at, sort_order, due_date, remind_at, remind_rule, is_recurring | 待办（本期可仅用基础字段） |
| `completion_log` | id, todo_item_id, sticker_id, text, completed_at | 已完成日志（可选） |
| `assets` | id, sticker_id, name, mime_type, file_path, file_size | 图片资源（可选） |
| `system_config` | key(PK), value, description, updated_at | 全局默认偏好 |
| `sticker_prefs` | sticker_id(PK), opacity, title_centered, title_font_size, body_font_size, bg_color, text_color, auto_scroll_speed | 单便签覆盖偏好（全可空） |

- 迁移机制：`PRAGMA user_version`（初版设为与源一致的版本号以便旧库直接接入），每步迁移用事务包裹、幂等（`IF NOT EXISTS` / `INSERT OR IGNORE`）。
- 连接：`Arc<Mutex<Connection>>` 单连接 + `spawn_blocking`；`journal_mode=WAL`、`foreign_keys=ON`。
- 路径：`app.path().app_data_dir()` 下 `stickers.db`。

---

## 6. 接口设计（命令 / 事件）

### 6.1 Tauri 命令（JS→Rust，示例签名）
- `list_stickers() -> Vec<StickerRow>`
- `get_sticker(id) -> StickerWithAttrs`
- `create_sticker(partial) -> i64`
- `update_sticker(id, patch: StickerPatch)`
- `delete_sticker(id)`
- `set_reminder(id, attrs: StickerAttrs)` / `clear_reminder(id)` / `acknowledge_alert(id)`
- `set_config(key, value)` / `get_config() -> SystemConfig`
- `update_sticker_prefs(id, prefs)` / `reset_sticker_prefs(id)`
- `toggle_todo(id, line: usize)`（line = Markdown 源行号，由前端 markdown-it 提供）
- `import_asset(id, path)`（可选）
- `slash_query(q) -> Vec<SlashCommand>`（检索，前端也可本地做）

### 6.2 事件（Rust→前端，经 `app.emit_to(label, event, payload)`）
- `sticky://push-update`：便签内容/属性变更推送（label = `sticker-<id>` 或 `main`）。
- `sticky://alert-active`：提醒触发状态信号（payload: bool，**无动画**，前端自行表现）。
- `sticky://prefs-updated`：偏好变更。

---

## 7. 关键技术要点

1. **多窗口 + 同构前端**：每个 `WebviewWindow` 加载同一 `index.html`，由窗口 label 区分主控台/便签；便签 id 经 `getCurrentWindow().label()` 解析。
2. **窗口样式**：`tauri.conf.json` 设默认 `decorations:false` + `transparent:true` + `alwaysOnTop`；运行时按 `display_mode`/偏好用 `Window` API 调 `set_skip_taskbar` / `set_always_on_top` / `set_ignore_cursor_events` / `start_dragging`。
3. **提醒调度**：`setup` 内 `tauri::async_runtime::spawn` 一个 10s `interval` 循环；到期 → `emit_to` `alert-active` + `tauri-plugin-notification` 弹通知；续期算法（以原 `remind_at` 为基准推进、追补关机缺口）全新实现。
4. **Markdown 渲染**：前端 `markdown-it` + task-lists；todo 勾选经 `token.map` 拿源行号 → `toggle_todo(id, line)`；CJK 排版用 CSS `word-break/overflow-wrap`。
5. **状态管理**：Rust `AppState`（`tauri::State`）持有内存缓存 + 连接；前端 `pinia` 镜像列表/偏好，命令返回即更新，事件用于跨窗口同步。
6. **单一 runtime**：全程使用 Tauri 内置 async runtime，禁建第二个 tokio runtime。
7. **slash 引擎**：`slash/*` 纯逻辑全新实现（命令表/匹配/插入/状态机）；前端负责浮层 UI 与键盘导航。
8. **前端工程规范**：严格遵循 §4.3 —— 大功能拆文件夹、子功能拆独立 `.vue`/`.rs`、单文件不过大；**全程使用 `v-model`（`defineModel`）做双向绑定**，pinia store 为单一真相源，组件经 `computed` get/set 桥接 Tauri 命令。库栈见 §3.2（pinia/router/vueuse/vuedraggable/dayjs/sass 等）。

---

## 8. 分阶段实施路线图（仅步骤，不编码）

- **阶段 0 — 脚手架**：在现有 Tauri 2 + Vue 3 模板上扩展；引入 `tauri-plugin-notification`/`autostart`；扩展 `capabilities/default.json`（加 `notification:default`、`autostart:default`、`core:tray:allow-*` 等）；`tauri.conf.json` 配置窗口样式。
- **阶段 1 — 数据层**：`models.rs` + `db/`（connection/schema/repos），`app_data_dir` 路径，跑通建库/迁移/CRUD。
- **阶段 2 — 核心逻辑**：`datetime/`（重复规则）、`slash/`、`editing/`、`commands.rs`。
- **阶段 3 — 平台集成**：`platform/tray.rs`、`notify.rs`、`autostart.rs`、`window_style.rs`（用 Tauri 原生 API）。
- **阶段 4 — 提醒调度**：`reminder/scheduler.rs` 在 `setup` 启动 + 通知 + 事件。
- **阶段 5 — Vue UI**：`Console.vue`、`StickerWindow.vue`、多窗口 label 路由、`MarkdownView.vue`（markdown-it）、`SlashMenu.vue`、`DateTimePicker.vue`、`stores/`、`composables/`。
- **阶段 6 — 集成联调**：多窗口创建/恢复、命令/事件打通、跨窗口状态同步、三模式与偏好应用。
- **阶段 7 — 测试与对等**：功能对照 `oi_sticker`（便签 CRUD、Markdown 等价、提醒续期、托盘/通知/自启、窗口样式）；旧 `stickers.db` 接入验证；编写单测（datetime/slash/db 迁移）。

---

## 9. 潜在风险与缓解

| # | 风险 | 严重度 | 缓解 |
|---|---|---|---|
| **R1** | **Markdown 渲染差异**：`markdown-it` 与源 `pulldown-cmark→Block` 自绘表现需对齐（尤其 todo、表格、图片、CJK）。 | 中 | 用 `markdown-it` + task-lists 插件；todo 源行映射改用 `token.map`；上线前做"同份源"渲染对照测试。 |
| **R2** | **多窗口状态同步**：多个 webview 间便签列表/偏好需一致。 | 中 | 后端为单一 `AppState` 真相源；变更经命令写库 + `emit_to` 广播事件；前端 pinia 监听更新。 |
| **R3** | **多窗口同构加载**：每个窗口如何知道渲染哪个便签。 | 中 | 统一 `index.html`，`App.vue` 据 `getCurrentWindow().label()` 分发；便签 id 从 label 解析。 |
| **R4** | **`remind_at` 时间字符串格式**：源曾混用 `YYYY-MM-DD HH:MM` 与 `YYYY-MM-DDTHH:MM:SS`。 | 中 | 新项目统一为一种格式（建议 ISO `YYYY-MM-DDTHH:MM:SS`），并提供旧数据兼容解析。 |
| **R5** | **capabilities 权限不足**：托盘/通知/自启/路径/多窗口需扩展权限。 | 中 | 在 `capabilities/default.json` 增加相应 `allow-*`（notification/autostart/core:tray/core:window 等）。 |
| **R6** | **旧数据库兼容**：是否/如何接入 `oi_sticker` 旧 `stickers.db`。 | 低 | 沿用相同 schema 设计与 `user_version`；提供导入/兼容读取路径（可在阶段 7 验证）。 |
| **R7** | **许可 GPL v3**：衍生须同协议。 | 低（已确认） | 保留 LICENSE 与作者声明，不换协议。 |
| **R8** | **编辑器智能行为**：缩进/续行/删除标记/todo 行定位需在（Rust 或前端）重新实现。 | 低 | 放在 `editing/` 纯函数（或前端 composable），由 `toggle_todo(id,line)` 等命令调用。 |

---

## 10. 验收标准（Done 定义）

- [ ] 启动后自动建库并跑通 schema 迁移；可选接入旧 `oi_sticker` 的 `stickers.db` 数据不丢失。
- [ ] 便签增/删/改/查、位置/尺寸/模式重启后恢复。
- [ ] 前端 `markdown-it` 正确渲染（标题/列表/任务清单/引用/链接/表格/图片），todo 勾选映射回源行并落库。
- [ ] 提醒按 `daily/weekly/interval:N/monthly:N/yearly:M-D` 正确续期、追补关机缺口，触发系统通知 + `alert-active` 状态（无动画）。
- [ ] 托盘菜单（新建/主控台/设置/退出）工作；自启可用。
- [ ] 窗口透明/无边框/置顶/任务栏隐藏/（可选）点击穿透按配置生效。
- [ ] slash 命令引擎（约 20 条 + 检索）可用；三模式语义与 oi_sticker 一致（display=低透明收起、无动画）。
- [ ] 不含 `slint`/`pulldown-cmark`/`tray-icon`/`notify-rust`/`windows-sys` 任何依赖；单一 Tauri runtime。
- [ ] GPL v3 声明保留。

---

## 11. 与旧方案的关系

- 本文件 **`REWRITE_PLAN.md` 取代 `MIGRATION_PLAN.md`**：方向由"Slint→Tauri 移植"改为"Tauri 2 全新重写，功能仿照 oi_sticker"。
- 前期对 `oi_sticker` 的分析结论（功能、数据模型、行为、设计意图）继续作为本方案的**需求规格**使用，无需重做分析。
- 旧的 `MIGRATION_PLAN.md` 可保留归档或删除（不影响新开发）。

> 本方案为审查草稿，未包含任何实现代码。确认后可进入"批准后编码"阶段，按 §8 阶段 0→7 落地。
