# 阶段 0 — 前期准备（脚手架）

> 所属总览：[`../PLAN.md`](../PLAN.md) · 状态：⏳ 未开始（下一步） · 完成标志：`pnpm tauri dev` 空窗口启动、依赖/权限/许可就位

## 0️⃣ 需要的 Skills / MCP（本阶段）
| 工具 | 用途 | 状态 |
|---|---|---|
| `reasonix-guide` | 排查技能/MCP 加载问题（若新装技能未生效） | ✅ 已装 |
| `install-capability` | 若还需安装/卸载 MCP 或技能 | ✅ 已装 |
| `context7-mcp`（MCP） | 查 Tauri 2 官方配置：`tauri.conf.json`、`capabilities`、插件安装 | ✅ 已装 |
| `Ref`（MCP） | 搜 Tauri 文档/GitHub 确认配置字段 | ✅ 已装 |
| `pnpm`（antfu skill） | 前端包管理规范（pnpm 工作流） | 🆕 新装（重启会话生效） |
| `init` | 阶段末尾生成 `AGENTS.md` 记录工程约定 | ✅ 已装 |
| `claude-vision-skill` | （可选）验证启动后的窗口截图 | 🆕 新装 |

## 📋 详细步骤
1. **Rust 依赖**（`src-tauri/Cargo.toml`）：
   - `tauri = { version = "2", features = ["tray-icon", "image-png"] }`（托盘后端 + 托盘图标）
   - `tauri-plugin-notification = "2"`、`tauri-plugin-autostart = "2"`
   - 预置（阶段 1 用）：`rusqlite = { version = "0.40", features = ["bundled"] }`、`anyhow = "1"`、`tracing` + `tracing-subscriber`
   - 确认 `tauri-plugin-opener` 保留或移除（源项目无 opener 需求，可删）
2. **前端依赖**（`package.json`）：
   - 运行时：`pinia`、`vue-router`、`@vueuse/core`、`dayjs`、`markdown-it`、`markdown-it-task-lists`（todo 勾选）、`@types/markdown-it`(dev)
   - 样式：`sass`（devDependency）
   - 测试：`vitest`、`@vue/test-utils`、`jsdom`（devDependency）
3. **capabilities 权限**（`src-tauri/capabilities/default.json`）：追加
   - `notification:default`、`autostart:default`
   - `core:default` 已含 `core:tray`/`core:window`/`core:event`/`core:path`；按需补 `core:window:allow-set-always-on-top`、`core:window:allow-set-skip-taskbar`、`core:window:allow-set-ignore-cursor-events`、`core:window:allow-start-dragging`、`core:webview:allow-create-webview-window`（多窗口）等
4. **窗口配置**（`tauri.conf.json`）：
   - `app.windows[0]`（主控台 `main`）：`decorations: false`、`transparent: true`、`width/height` 主控台尺寸；`alwaysOnTop` 默认 false
   - `identifier` 规范化；`bundle` 段设置 app 名称/图标占位
5. **许可合规**：从 `E:\AAACodeProjectMy\oi_sticker\LICENSE` 复制 GPL v3 文本到本工程根目录，保留原版权/作者声明；README 标注"衍生自 oi_sticker（GPL v3）"
6. **工程卫生**：`.gitignore` 核对（`node_modules`、`src-tauri/target`、`dist`）；删除模板残留（`greet` 命令、`vue.svg`、欢迎页）
7. **AGENTS.md**：用 `init` 生成，记录构建/测试命令、目录结构、红线
8. **验证**：`cargo check`（src-tauri）、`pnpm install && pnpm build`、`pnpm tauri dev` 能启动空窗口

## ✅ 验收
- [ ] `pnpm tauri dev` 正常启动；`cargo check` 无错误
- [ ] capabilities 含通知/自启/多窗口权限；Cargo.toml 无 slint/pulldown-cmark/tray-icon/notify-rust/windows-sys
- [ ] GPL v3 LICENSE 就位；AGENTS.md 生成
- [ ] 新装 skills 在重启后的会话中可被 `/` 列出

## 完成动作
- 更新 `../PLAN.md` §1 状态表 → 下一步：验证 Demo
- **中文 git 提交**，如：`feat: 完成阶段0脚手架（依赖/权限/许可/工程初始化）`
