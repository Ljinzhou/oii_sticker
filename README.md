# oii_sticker

Tauri 2 + Vue 3 + TypeScript 桌面便签应用（重写自 [oi_sticker](https://github.com/)，GPL v3）。

多窗口 Markdown 便签：三模式（展示/交互/编辑）、SQLite 持久化、提醒调度（重复规则 + 关机追补）、系统托盘、通知、开机自启、窗口样式个性化、slash 命令。

## 功能

- **多便签多窗口**：每个便签一个独立 OS 窗口（透明、无边框、可置顶、任务栏隐藏）
- **三种模式**：display（低透明收起，双击唤醒）/ interact（可勾选待办，5s 无操作自动收起）/ edit（编辑器 + slash 命令）
- **Markdown 渲染**：前端 markdown-it（标题/列表/任务清单/引用/链接/表格/图片），todo 勾选映射回源行并落库
- **提醒调度**：`daily` / `weekly:mon,wed` / `interval:N` / `monthly:N` / `yearly:M-D` + 旧格式（`30m`/`1h`/`2d`），触发系统通知 + `alert-active` 状态信号（无动画）
- **个性化偏好**：背景色/透明度/字号/置顶/自动滚动，合并链 `prefs → sticker → system → 兜底`
- **系统集成**：托盘（新建/主控台/设置/退出）、通知（官方插件）、开机自启（官方插件）
- **slash 命令**：20 条内置（标题/列表/代码/高级），拼音/首字母/别名/中文检索

## 开发

```powershell
pnpm install          # 安装前端依赖
pnpm tauri dev        # 开发运行
pnpm test             # 前端 vitest
pnpm build            # vue-tsc + vite build
cd src-tauri
cargo test            # Rust 单测（99）
cargo clippy          # lint
pnpm tauri build      # 打包
```

## 文档

- `docs/PLAN.md` — 执行总览与进度追踪
- `docs/plan/phase-*.md` — 各阶段详细计划
- `docs/parity-matrix.md` — 与旧项目行为对照矩阵
- `AGENTS.md` — 工程约定

## 技术红线（已达成）

- 不含 `slint` / `pulldown-cmark` / `tray-icon` / `notify-rust` / `windows-sys` 直调
- 单一 Tauri runtime（无独立 tokio runtime）
- SQLite（rusqlite bundled），schema 与旧库兼容（`user_version=5`）
- GPL v3（保留原 LICENSE 与作者声明）

## 许可

GNU General Public License v3.0（衍生自 oi_sticker，保留原版权与作者声明）。
