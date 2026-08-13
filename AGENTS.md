# AGENTS.md — oii_sticker 工程约定

> 本文件供 AI 代理与开发者阅读。执行计划见 `docs/PLAN.md`（总览+进度）与 `docs/plan/phase-*.md`（各阶段详细 plan）。

## 项目简介

Tauri 2 + Vue 3 + TypeScript 桌面便签应用（重写自 `oi_sticker`，GPL v3）。多窗口 Markdown 便签、三模式（display/interact/edit）、SQLite 持久化、提醒调度、托盘/通知/自启、slash 命令。

## 常用命令

```powershell
pnpm install                  # 安装前端依赖
pnpm tauri dev                # 开发运行（Rust 编译 + Vite dev）
pnpm build                    # vue-tsc 类型检查 + vite build
pnpm test                     # vitest 前端测试（配置后）
cd src-tauri
cargo check                   # Rust 快速检查
cargo test                    # Rust 单测
cargo clippy                  # Rust lint
```

## 目录结构

```
docs/PLAN.md                  总览 + 执行进度追踪
docs/plan/phase-*.md          各阶段独立 plan
src/                          前端（Vue 3 + TS）
  styles/global.css           全局样式（html/body 透明）
  App.vue                     窗口 label 分发（占位 → demo 实现）
src-tauri/                    Rust 侧
  src/lib.rs                  Tauri Builder + 插件 + 命令注册
  capabilities/default.json   权限（windows: main + sticker-*）
  tauri.conf.json             窗口配置（transparent/decorations:false）
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
- **敏感文件**：`docs/test.jpg`（身份证测试图）、`.env` 不得提交
