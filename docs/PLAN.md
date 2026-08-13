# oii_sticker 开发总览（docs/PLAN.md）

> 版本：v1.1（2026-08-13）
> 依据：`REWRITE_PLAN.md`（v1.0，已确认取代 `MIGRATION_PLAN.md`，方向为 **Tauri 2 全新重写**，旧项目 `oi_sticker` 仅作需求规格）
> 本文档是**执行总览**：记录项目目标、红线、工具清单与 **当前执行到哪个阶段**。每个阶段的详细实现 plan 见 `docs/plan/phase-*.md`（每个阶段一个独立文件，文件开头固定列出该阶段要使用的 Skills / MCP 组合）。

---

## 1. 执行状态追踪（每完成一个阶段更新这里）

| 阶段 | 独立 plan | 状态 |
|---|---|---|
| 阶段 0 — 前期准备（脚手架） | [`docs/plan/phase-0.md`](plan/phase-0.md) | ✅ 已完成（2026-08-13） |
| 验证 Demo — 多窗口背景半透明 | [`docs/plan/phase-demo.md`](plan/phase-demo.md) | ✅ 已完成（2026-08-13，视觉验证通过） |
| 阶段 1 — 数据层（models + db） | [`docs/plan/phase-1.md`](plan/phase-1.md) | ✅ 已完成（2026-08-13，13 单测全绿） |
| 阶段 2 — 核心逻辑（datetime / slash / editing / commands） | [`docs/plan/phase-2.md`](plan/phase-2.md) | ⏳ 未开始（**下一步**） |
| 阶段 3 — 平台集成（tray / notify / autostart / window_style） | [`docs/plan/phase-3.md`](plan/phase-3.md) | ⏳ 未开始 |
| 阶段 4 — 提醒调度（reminder/scheduler） | [`docs/plan/phase-4.md`](plan/phase-4.md) | ⏳ 未开始 |
| 阶段 5 — Vue UI（全新设计） | [`docs/plan/phase-5.md`](plan/phase-5.md) | ⏳ 未开始 |
| 阶段 6 — 集成联调（多窗口 + 命令 + 事件） | [`docs/plan/phase-6.md`](plan/phase-6.md) | ⏳ 未开始 |
| 阶段 7 — 测试与对等性验证 | [`docs/plan/phase-7.md`](plan/phase-7.md) | ⏳ 未开始 |

> **更新规则**：每完成一个阶段 → 更新上表状态为 ✅ 已完成、把"下一步"指向下一个阶段 → **用中文写 git 提交信息提交**（每个完成的小块功能单独提交，见 §5）。

## 2. 目标与红线

### 2.1 目标
在 `E:\AAACodeProjectMy\oii_sticker`（Tauri 2 + Vue 3 + TS）上从零实现 `oi_sticker`（Slint 便签）的功能对等品：多窗口 Markdown 便签、三模式（display/interact/edit）、SQLite 持久化、提醒调度、托盘/通知/自启、窗口样式个性化、slash 命令。**UI 完全用 Vue 3 重新设计**。

### 2.2 技术红线（全阶段有效）
- **不出现**：`slint` / `pulldown-cmark` / `tray-icon` crate / `notify-rust` / `windows-sys` 直调。
- **单一 runtime**：只用 Tauri 内置 `tauri::async_runtime`，禁止再建 tokio runtime。
- **Tauri 原生优先**：窗口/托盘/通知/自启全部用 Tauri 2 API 或官方插件。
- **数据库**：SQLite（`rusqlite` bundled），沿用源 schema 设计 + `user_version=5` 迁移机制，旧 `stickers.db` 可接入 `app_data_dir`。
- **无动画**：`alert_active` 仅作状态信号。
- **许可**：GPL v3，保留 LICENSE 与作者声明。
- **提交规范**：功能开发每完成一小块，用**中文** git 提交信息提交。

## 3. 多 Agent 策略（必要时启动多 agent）

| 场景 | 用哪个 | 何时用 |
|---|---|---|
| 分析旧项目模块、提取需求规格 | `explore` 子代理（`tool:task`） | 阶段 1/2/7 开始前，分模块并行派发 |
| 2+ 个独立子任务（如并行实现多个 repo / 多个 Vue 组件） | `dispatching-parallel-agents` | 阶段 1（4 个 repo）、阶段 5（多组件） |
| 代码审查 | `tool:review` / `security_review` | 每阶段完成后的 checkpoint |
| 疑难 bug | `systematic-debugging` / `diagnose` | 阶段 6 联调 |
| 与外部文档/代码对照验证 | `tool:research` | 涉及 Tauri API 行为疑问时 |
| **视觉验证/看图调试** | `claude-vision-skill`（硅基流动） | 程序界面验证、截图分析、图片相关 debug（见 §4.3） |

## 4. Skills / MCP 工具清单

### 4.1 本阶段状态标记
- ✅ 已装（可用）
- 🆕 新装（需**重启会话**后才会出现在 `/` 命令列表；当前会话内可直接按文件内容执行）

### 4.2 已安装列表
| Skills / MCP | 来源 | 状态 |
|---|---|---|
| `context7-cli` / `context7-mcp`（MCP server 已配置） | 本地已有 | ✅ 已装 |
| `using-superpowers`、`frontend-design` | 本地已有 | ✅ 已装 |
| `taste-skill` | Leonxlnx/taste-skill（MIT，76k⭐） | 🆕 新装 |
| `impeccable` | DevvGwardo/impeccable（23 commands UI 设计/评审） | 🆕 新装 |
| `huashu-design` | alchaincyf/huashu-design（MIT，22.9k⭐，中文） | 🆕 新装 |
| `antfu` / `antfu-design` | antfu/skills（MIT） | 🆕 新装 |
| `vue` / `pinia` / `vite` / `vitest` / `pnpm` / `unocss` | antfu/skills | 🆕 新装 |
| `vueuse-functions` / `vue-best-practices` / `vue-router-best-practices` / `web-design-guidelines` | antfu/skills（vendor 子集） | 🆕 新装 |
| `claude-vision-skill` | asuojun/claude-vision-skill（已适配硅基流动） | 🆕 新装 |
| MCP：`Ref`（configured）、`ssh-mcp-server`（configured） | 本地已有 | ✅ 已装 |
| 流程类：`tdd`、`test`、`executing-plans`、`writing-plans`、`review`、`explore`、`install-capability`、`reasonix-guide`、`init`、`systematic-debugging`、`verification-before-completion` 等 | 本地已有 | ✅ 已装 |

### 4.3 视觉验证（claude-vision-skill）使用方法
- 安装位置：`C:\Users\ljz\.agents\skills\claude-vision-skill\`
- 配置：`.env`（`DASHSCOPE_API_KEY` / `VISION_MODEL=nex-agi/Nex-N2-Pro` / `DASHSCOPE_BASE_URL=https://api.siliconflow.cn/v1`）
- 用法：
  ```powershell
  node C:\Users\ljz\.agents\skills\claude-vision-skill\vision.js "<图片绝对路径>" "<问题>"
  node C:\Users\ljz\.agents\skills\claude-vision-skill\vision.js --clipboard "<问题>"
  node C:\Users\ljz\.agents\skills\claude-vision-skill\vision.js --url "<图片URL>" "<问题>"
  ```
- 适用：UI 截图验证、程序运行效果检查、图片相关功能 debug。**注意**：身份证等含敏感信息的图片，向用户汇报时对敏感字段脱敏。
- 已知适配点：`Nex-N2-Pro` 为推理模型，`max_tokens` 已调至 4096 避免正文被推理截断；如输出仍为空，继续调大 `max_tokens`。

## 5. 提交与记忆规范

- **git 提交**：每完成一小块功能开发（如"完成 db 连接与建库迁移"），用**中文提交信息**提交（`git add -A && git commit -m "中文说明"`）。
- **记忆存储**：遇到需要长期记住的项目规则/决策/坑，用 `remember` 工具存为后台记忆（scope=project 或 global）。
- **进度更新**：阶段完成时更新 §1 表格并提交。

## 6. 验收总纲（阶段 7 最终核对，详见各阶段文件）

- 启动自动建库 + `user_version=5` 迁移；旧 `stickers.db` 兼容
- 便签增/删/改/查、偏好合并；三模式语义正确（display=低透明收起、无动画）
- 前端 `markdown-it` 渲染 + todo 源行映射回写；提醒按 5 种重复规则续期 + 追补
- 托盘/通知/自启/窗口样式按配置生效；slash 约 20 条命令可用
- 依赖红线复核：无 slint/pulldown-cmark/tray-icon/notify-rust/windows-sys；单一 runtime
- GPL v3 声明保留

## 附录 A：常用命令速查
```powershell
pnpm install            # 前端依赖
pnpm tauri dev          # 开发运行（编译 + 启动）
pnpm build              # vue-tsc + vite build
cd src-tauri; cargo check / cargo test / cargo clippy   # Rust 侧
```

## 附录 B：注意事项
- `docs/test.jpg` 为身份证测试图片（含证件信息），**不要提交到 git 仓库**（或验证后删除）。
- 新装的全局 skills 重启会话后生效；本会话内可按 `C:\Users\ljz\.agents\skills\<name>\SKILL.md` 直接执行。
