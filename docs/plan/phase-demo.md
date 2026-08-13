# 验证 Demo — 多窗口背景半透明（进入 Phase 1 前的核心功能验证）

> 所属总览：[`../PLAN.md`](../PLAN.md) · 状态：⏳ 未开始 · 目的：提前验证**最核心技术风险**（多窗口模型 + 窗口透明 + 前端独立状态），跑通后其窗口创建命令 + label 分发 + `StickerWindow.vue` 骨架直接作为阶段 1-5 的基座

## 🎯 需要的 Skills / MCP（本阶段）
| 工具 | 用途 | 状态 |
|---|---|---|
| `context7-mcp`（MCP） | 查 Tauri 2 精确 API：`WebviewWindowBuilder`、`transparent`/`decorations`/`skip_taskbar`、Windows 平台透明窗口注意事项 | ✅ 已装 |
| `Ref`（MCP） | 搜 Tauri 官方文档/issue（如透明窗口黑边、resize 闪烁问题） | ✅ 已装 |
| `frontend-design` | demo 界面基础设计（主控台 + 便签卡片布局） | ✅ 已装 |
| `taste-skill` + `impeccable` + `huashu-design` + `antfu-design` + `web-design-guidelines` | 界面"品味"把关：配色、滑块控件、卡片质感，避免 AI 味 | 🆕 新装（重启会话生效） |
| `vue` + `vite`（antfu skill） | Vue 组件写法、Vite 配置正确性 | 🆕 新装 |
| `pinia`（antfu skill） | 每窗口状态管理（demo 可先不用，预留） | 🆕 新装 |
| `systematic-debugging` | 透明窗口异常（黑底/黑边/穿透失效）排查 | ✅ 已装 |
| `test` | 构建/类型检查验证 | ✅ 已装 |
| `claude-vision-skill` | 启动后**截图验证**界面效果（半透明/文字清晰） | 🆕 新装 |

## 🔬 技术方案（关键设计，写死进代码）
1. **窗口模型**（与 REWRITE_PLAN §4.4 一致）：
   - 主控台窗口 label = `main`；便签窗口 label = `sticker-<n>`（n 自增）
   - Rust 命令 `create_sticker_window(title: String, x: f64, y: f64) -> String`（返回 label）：
     ```rust
     WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html".into()))
         .title(title)
         .inner_size(320.0, 240.0)
         .position(x, y)
         .transparent(true)      // 关键：窗口透明
         .decorations(false)     // Windows 透明窗口需无边框
         .skip_taskbar(true)
         .resizable(true)
         .build()
     ```
   - 主控台注册 `create_sticker_window`、`list_sticker_windows`（返回当前窗口 label 列表）两个命令
2. **前端同构分发**（`App.vue`）：`getCurrentWindow().label()` 以 `sticker-` 开头 → 渲染 `StickerWindow.vue`；否则渲染 `Console.vue`（新建按钮 + 已开窗口列表）
3. **背景半透明、文字不透明**（核心实现）：
   - `index.html`：`html, body, #app { background: transparent; }` —— 窗口透明，桌面透过来
   - `StickerWindow.vue` 根卡片：`background: rgba(255, 244, 214, var(--alpha))`，`--alpha` 由滑块驱动（0.15~1.0）
   - 文字/内容元素：**不设任何透明度**（默认不透明）→ 天然满足"背景半透明、文字不透明"
   - 滑块：`<input type="range" min="15" max="100" v-model="alphaPct">` + `computed` 输出 CSS 变量；滑块本身放在不透明的小工具条上（否则滑块也透明）
4. **独立调整**：每个 `sticker-<n>` 窗口是独立 Vue 实例（独立 webview），`alphaPct` 是组件内 `ref` —— **天然互不影响**
5. **拖动**（无边框窗口）：根元素加 `data-tauri-drag-region`（Windows 上 Tauri 原生支持）
6. **Windows 已知坑（写入代码注释/文档）**：
   - 透明窗口需 `decorations(false)`；resize 时可能短暂黑边/闪烁（WebView2 限制，demo 阶段不处理）
   - 不要对 `body` 整体设半透明背景——会把文字一起变透明，正确做法是"容器卡片 rgba + 文字不透明"

## 📋 详细步骤
1. `tauri.conf.json` 主窗口加 `transparent: true, decorations: false`；capabilities 补 `core:window:allow-*`（若 demo 需要 set_always_on_top 等）
2. Rust：`lib.rs` 注册 `create_sticker_window` / `list_sticker_windows` 命令；删除 `greet`
3. 前端：`App.vue` label 分发；新建 `src/components/demo/Console.vue`、`src/components/demo/StickerWindow.vue`
4. 样式：`src/styles/demo.scss`（或直接 scoped style），滑块样式用 `taste-skill`/`frontend-design` 打磨
5. 验证：`pnpm tauri dev` → 主控台点"新建便签"×3 → 三个窗口位置错开 → 各窗口滑块独立拖动 → 观察桌面透过背景、文字清晰
6. **截图验证**：用 `claude-vision-skill` 对运行中的窗口截图做视觉确认（背景半透明、文字不透明、滑块可见）

## ✅ 验收（Demo 的 Done 定义）
- [ ] 可创建多个独立便签窗口（≥3），互不干扰，可拖动、可关闭
- [ ] 每个窗口有独立滑块，调整只影响本窗口背景透明度
- [ ] 背景半透明时**文字不透明**（标题/正文清晰可读）；桌面内容可透过背景可见
- [ ] 窗口重启（关闭后重新 `tauri dev`）仍能创建
- [ ] 代码不含 slint/pulldown-cmark/tray-icon/notify-rust/windows-sys；单一 Tauri runtime
- [ ] 视觉 skill 截图确认通过

## 完成动作
- 更新 `../PLAN.md` §1 状态表 → 下一步：阶段 1
- **中文 git 提交**，如：`feat: 验证Demo完成（多窗口背景半透明+独立透明度滑块）`
