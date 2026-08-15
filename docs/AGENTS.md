# AGENTS.md — oii_sticker 工程约定

> 本文件供 AI 代理与开发者阅读。执行计划见 `docs/PLAN.md`（总览+进度）与 `docs/plan/phase-*.md`（各阶段详细 plan）。

## 项目简介

Tauri 2 + Vue 3 + TypeScript 桌面便签应用（重写自 `oi_sticker`，GPL v3）。多窗口 Markdown 便签、三模式（display/interact/edit）、SQLite 持久化、提醒调度、托盘/通知/自启、slash 命令。

## 常用命令

```powershell
pnpm install                  # 安装前端依赖
pnpm tauri dev                # 开发运行（Rust 编译 + Vite dev）
pnpm test                     # vitest 前端测试（9 用例）
pnpm build                    # vue-tsc 类型检查 + vite build
pnpm tauri build --no-bundle  # 打包冒烟（release 编译）
cd src-tauri
cargo check                   # Rust 快速检查
cargo test                    # Rust 单测（99）
cargo clippy                  # Rust lint（0 警告）
```

## 目录结构

```
docs/PLAN.md                  总览 + 执行进度追踪（阶段 0-6 已完成，7 进行中）
docs/plan/phase-*.md          各阶段独立 plan
docs/parity-matrix.md         与旧项目行为对照矩阵
src/                          前端（Vue 3 + TS + pinia）
  stores/                     notes/settings/prefs
  components/console/         主控台（列表 + 设置面板）
  components/sticker/         便签窗口（三模式容器/头部/查看/编辑/设置）
  components/markdown/        markdown-it 渲染（todo 源行映射）
  components/slash/           slash 浮层
  utils/markdown.ts           markdown-it 实例 + hexToRgba
src-tauri/                    Rust 侧
  src/lib.rs                  Builder + 15 个业务命令 + 托盘/调度器 setup
  src/db/                     schema v1-v5 迁移 + 5 个 repo
  src/datetime/               自研 DateTime + 重复规则
  src/slash/                  20 条命令 + 匹配/插入/状态机
  src/editing/                编辑器纯函数（Tab/Enter/Backspace/todo）
  src/reminder/scheduler.rs   10s 调度 + 续期追补
  src/platform/               tray/notify/autostart/window_style
  src/events.rs               事件名常量 + emit 封装
```

## 技术红线（违反即失败）

- 不出现 `slint` / `pulldown-cmark` / `tray-icon` / `notify-rust` / `windows-sys` 直调
- 单一 Tauri runtime（`tauri::async_runtime`），禁止第二个 tokio runtime
- 窗口/托盘/通知/自启用 Tauri 2 原生 API 或官方插件
- SQLite（rusqlite bundled），沿用源 schema + `user_version=5` 迁移
- 无动画：`alert_active` 仅状态信号
- GPL v3：保留 LICENSE 与作者声明

## 工作约定

- **git 提交**：每完成一小块功能，用**中文**提交信息提交（`git add -A && git commit -m "中文说明"`）
- **阶段进度**：完成一个阶段 → 更新 `docs/PLAN.md` §1 状态表 → 提交
- **视觉验证**：需要看图（UI 截图/运行效果/debug）时用 `claude-vision-skill`（全局，硅基流动 nex-agi/Nex-N2-Pro）
- **Python 辅助**：内置工具不足时可编写 Python 脚本辅助 debug/开发，用完清理
- **敏感文件**：`.env` 不得提交

