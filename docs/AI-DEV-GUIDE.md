# AI 开发规范指引（oii_sticker）

> 本文件是 AI 代理在本项目开发时的**强制规范**：已存储记忆、开发注意事项、以及"开发什么功能前必须使用什么 Skills"。
> 位置：`docs/AI-DEV-GUIDE.md` · 配合 `AGENTS.md`（工程约定）与 `docs/PLAN.md`（进度总览）使用。

---

## 1. 已存储的项目记忆（后台记忆，跨会话生效）

| 记忆名 | 类型 | 内容 |
|---|---|---|
| `git-commit-chinese` | feedback | **每完成一小块功能开发，立即用中文写 git 提交信息并提交**（`git add -A && git commit -m "中文说明"`）。示例：`feat: 完成数据层（models/db/迁移/单测全绿）` |
| `oii-sticker-vision-workflow` | project | **视觉辅助流程**：任何截图/图片判断必须用 `claude-vision-skill`（全局 `C:\Users\ljz\.agents\skills\claude-vision-skill\vision.js`），已适配硅基流动（模型 `nex-agi/Nex-N2-Pro`、端点 `https://api.siliconflow.cn/v1`、key 在 `.env`）。用法：`node <vision.js> "<图片路径>" "<问题>"`；max_tokens 已调 4096；含敏感信息图片（如身份证）汇报需脱敏 |
| `python-script-helper` | feedback | **内置工具不足时允许编写 Python 脚本辅助 debug/开发**（解析日志/转换数据/调用 API 等），用完清理，不留仓库 |
| `dev-workflow-constraints` | feedback | **① 等待后台任务超时 ≤ 30 秒**（不要用 90 秒级长等待，任务未完成就分批继续）；**② 截图未捕获程序窗口时直接通知用户**（用户可能正用电脑，其他窗口遮挡），不反复重试截图 |

---

## 2. 开发注意事项（红线与已知坑）

### 2.1 技术红线（违反即失败）
- **不出现** `slint` / `pulldown-cmark` / `tray-icon` / `notify-rust` 直调。
- **`windows-sys` 直调默认禁止**，唯一例外：`src-tauri/src/platform/mouse_hook.rs`（WH_MOUSE_LL 鼠标钩子，用户已明确批准，用于 display 全穿透 + 中键+左键唤醒）。
- **单一 Tauri runtime**：只用 `tauri::async_runtime`，禁止第二个 tokio runtime。
- 窗口/托盘/通知/自启用 Tauri 2 原生 API 或官方插件（`tauri-plugin-notification` / `tauri-plugin-autostart`）。
- SQLite（`rusqlite` bundled），沿用源 schema + `user_version=5` 迁移；旧 `stickers.db` 兼容。
- **无动画**：`alert_active` 仅状态信号；模式切换提示用应用内 toast（2 秒），不用系统通知。
- GPL v3：保留 LICENSE 与作者声明。

### 2.2 已知技术坑（务必注意）
| 坑 | 说明 |
|---|---|
| **Tauri 2 invoke 参数 camelCase** | Rust 命令参数 `is_display` 在前端必须传 `isDisplay`（否则报 `missing required key isDisplay` 且被 catch 静默吞掉）。**所有多词参数命令都要检查**。 |
| **事件 payload 类型匹配** | `emit_to(label, event, payload)` 的 payload 与前端 `listen<T>` 的 T 必须一致；payload 为 `()` 时前端不能解构 id。 |
| **Windows 上 Tauri 同步建窗会卡死** | `WebviewWindowBuilder::build` 不可在同步命令或 `run_on_main_thread` 的主线程任务内调用；命令必须声明为 `async`，并在其工作线程中建窗（Tauri 2 / WebView2 官方建议）。数据库访问使用 `with_conn_async`。 |
| **`webview_windows()` 含隐藏窗口** | 判断"窗口是否打开"要用 `win.is_visible()`，否则隐藏的便签仍被判为打开。 |
| **display 穿透与窗口内事件互斥** | `set_ignore_cursor_events(true)` 后窗口收不到任何鼠标事件；唤醒必须走全局鼠标钩子（中键武装 + 左键点击）。 |
| **display 锁尺寸双保险** | 展示模式除 `set_resizable(false)` 外，还要 `set_min_size/max_size` 锁死当前尺寸（防边缘拖拽）。 |
| **窗口状态在 setup 同步应用** | 启动恢复便签时按 `display_mode` 立即设置穿透/锁尺寸，不依赖前端加载完成。 |
| **GBK/UTF-8 文件** | 前端 `.vue` 为 UTF-8；PowerShell 读取中文可能乱码，用 `[System.Text.Encoding]::UTF8.GetString([IO.File]::ReadAllBytes(...))` 精确读取后再编辑。 |
| **PrintWindow 小 UI 不明显** | 验证小按钮/toast 时，截图后**裁剪放大目标区域**再交给视觉模型判断。 |
| **残留进程/端口** | 杀 tauri dev 后 exe/vite 可能残留（端口 1420 占用）→ 先 `Stop-Process` + 清端口再启动，否则测到旧代码。 |

### 2.3 模式与交互语义（当前实现）
- 三模式：`display`（默认，低透明收起、全穿透、锁尺寸）/ `interact`（顶部透明蒙版四按钮 ▽✎⚙✕、可拖动、无操作 N 秒自动收起，N 可在系统设置改，默认 5）/ `edit`（底部工具条：保存/取消/⚙/✕）。
- **唤醒协议**：任意位置按一次**鼠标中键**（武装，3 秒有效）→ **左键点击展示模式便签** → 唤醒进入 interact（钩子 Rust 侧取消穿透 + 发 `sticky://wake` 事件，前端切模式 + 2 秒 toast）。
- **自动收起前提**：仅 interact 且未打开设置面板；编辑模式/设置打开时不收起。
- 标题 = Markdown 首行 `# xxx`（h1 渲染），无独立标题字号设置。

---

## 3. Skills 使用指引（开发什么功能前用什么）

> 原则：涉及对应场景**必须**先加载/调用对应 Skills（1% 可能适用就要调用）。所有 Skills 在全局 `C:\Users\ljz\.agents\skills\`，重启会话后 `/` 命令可见。

| 开发场景 | 必须使用的 Skills / MCP |
|---|---|
| **任何截图、用户发图、图片判断** | `claude-vision-skill`（唯一看图途径） |
| **页面样式修改与设计** | `using-superpowers`、`frontend-design`、`design-taste-frontend`（taste-skill）、`impeccable`、`huashu-design`（+ 可选 `antfu-design`、`web-design-guidelines`） |
| **Vue 前端功能开发** | `vue`、`pinia`、`vite`、`vueuse-functions`、`vue-best-practices`、`vue-router-best-practices`（antfu skills） |
| **Rust/后端功能开发** | `using-superpowers`（流程）、`tdd` / `test-driven-development`（先写测试） |
| **测试与验证** | `tdd`、`test-driven-development`、`test`、`vitest`、`verification-before-completion`（完成声明前证据核验） |
| **Bug 修复 / 疑难排查** | `systematic-debugging`、`diagnose`（先复现→定位→修复→回归） |
| **库/框架文档查询** | `context7-mcp`（Context7 MCP）、`context7-cli`；`Ref` MCP（文档/源码搜索） |
| **Web 搜索** | `tavily-search` |
| **多 Agent 并行分析** | `explore`（只读探索）、`dispatching-parallel-agents`（并行子任务）、`research`（外文+代码对照） |
| **执行书面计划** | `executing-plans`、`subagent-driven-development`、`writing-plans` |
| **代码审查** | `review`、`requesting-code-review`、`receiving-code-review`、`security_review` |
| **需求/PRD/Issue** | `brainstorming`（创作前）、`to-prd`、`to-issues`、`triage` |
| **git 工作流** | `using-git-worktrees`、`finishing-a-development-branch` |
| **SQL/Schema** | `sql-optimization` |

### 3.1 Skills 加载方式
- 内联 Skills（如 `frontend-design`、`design-taste-frontend`、`huashu-design`）：`run_skill` / `read_skill` 加载后按其规范执行。
- 子代理类（`explore` / `research` / `review`）：直接用 `tool:task`（profile 指定）或 `tool:review`。
- 未安装的 Skills：先按 `install-capability` 安装到全局（`C:\Users\ljz\.agents\skills\`），重启会话生效。

---

## 4. 工作流程约定

1. **提交**：每完成一小块功能 → 中文提交（见 §1 `git-commit-chinese`）。
2. **进度**：完成一个阶段 → 更新 `docs/PLAN.md` §1 状态表 → 提交。
3. **验证**：构建/测试命令见 `AGENTS.md`；UI 验证必须截图 + `claude-vision-skill` 判断；截图未捕获程序窗口 → 直接告知用户。
4. **等待**：`wait` / 后台任务超时 ≤ 30 秒。
5. **临时脚本**：Python 辅助脚本用完清理，不留仓库。
6. **敏感信息**：`.env` 不提交；`docs/test.jpg`（身份证）等敏感图片不提交、汇报脱敏。

---

## 5. 常用命令速查

```powershell
pnpm install          # 前端依赖
pnpm tauri dev        # 开发运行
pnpm test             # vitest 前端测试
pnpm build            # vue-tsc + vite build
cd src-tauri
cargo check / cargo test / cargo clippy   # Rust 侧
pnpm tauri build --no-bundle              # 打包冒烟
```
