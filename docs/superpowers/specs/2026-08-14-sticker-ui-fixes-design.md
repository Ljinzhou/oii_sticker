# 设计：便签 UI 三项修复（2026-08-14）

> 状态：已获用户批准（方案 + 防御层 + 编辑工具条 ✕ 保留）
> 依据：`img/1.png`（claude-vision-skill 确认）、`docs/AI-DEV-GUIDE.md`、`AGENTS.md`

## 1. 需求

1. 移除主控台卡片上的 "edit" 模式徽章（蓝色小标签，显示 `display_mode` 字符串）。
2. 便签进入交互模式后，移除顶部蒙版的关闭窗口按钮（✕）。
3. 修复：便签窗口点 ✕ 关闭后，主控台「隐藏」按钮未变「显示」；再点「隐藏」变「显示」后，点「显示」无任何响应、无日志。

## 2. 根因分析（任务 3，systematic-debugging Phase 1 已核实）

1. `StickerWindow.vue:141-144`：`onClosed` 调 `getCurrentWindow().close()` —— **销毁**窗口而非隐藏。
2. `ConsoleView.vue:58-60`：主控台 `openIds` 仅在挂载（:84）与自身 `toggleSticker`（:51）后刷新；便签侧关闭不产生任何事件，主控台状态不同步。
3. `lib.rs:432-438 / 402-413`：`hide_sticker_cmd` 与 `wake_sticker_cmd` 均以 `if let Some(win) = get_webview_window(...)` 短路；窗口销毁后静默 no-op，`wake_sticker_cmd` 的 `info!` 日志在窗口不存在时不会执行 → 与用户观察「无日志」一致。

## 3. 方案

### 3.1 前端

**`src/components/console/ConsoleView.vue`**
- 删除 `<span class="mode-badge">{{ reminderText(s) }}</span>`（:118）与 `reminderText()`（:67-70）、`.mode-badge` 样式（:276-282）。—— 需求 1
- `push-update` 监听（:86）从仅 `notes.refresh()` 改为同时 `refreshOpenIds()`，使便签隐藏/新建等 Rust 侧广播都能同步按钮状态。

**`src/components/sticker/StickerWindow.vue`**
- 删除 interact 蒙版中的 ✕ 按钮（:198）。蒙版保留 ▽ / ✎ / ⚙ 三按钮。—— 需求 2
- `onClosed`（:141-144）改为 `await invoke("hide_sticker_cmd", { id: stickerId })`（隐藏不销毁，配合 3.2 的同步）。编辑模式工具条 ✕（StickerEditor.vue:128）保留，同样走该路径。

### 3.2 Rust（`src-tauri/src/lib.rs`）

- `hide_sticker_cmd`：隐藏成功后 `events::emit_push_update(&app, id)`，驱动主控台刷新 `openIds`（现有 push-update 广播机制复用，不新增事件名）。
- `wake_sticker_cmd`（防御层，用户已批准）：窗口不存在时从 DB 读取该便签（`commands::get_sticker`）并按 `create_sticker_win` 同参数重建窗口，再执行唤醒；补 `info!` 日志（重建 / 唤醒均记）。若 DB 无该便签则 `warn!` 并返回错误（不再静默）。

### 3.3 语义变化说明

- 便签窗口「关闭」从**销毁**变为**隐藏**：数据保留、可经主控台「显示」恢复；与「关闭不删数据」既有约定一致。
- 主控台「显示」现在总能生效：窗口在 → 唤醒；窗口被销毁 → 重建并显示。

## 4. 测试策略（TDD）

- **Rust**：`hide/wake` 依赖真实窗口运行时，不做 AppHandle mock 单测；验证靠 `cargo test`（既有 99 例不回归）+ `cargo clippy`。
- **前端 vitest**：新增 `ConsoleView.test.ts`（mock `@tauri-apps/api` 与 useTauri）：
  1. `push-update` 事件到达时同时调用 `refreshOpenIds`（列表 + openIds 双刷新）。
  2. 卡片不渲染 mode 徽章（无 `display_mode` 文本标签）。
  3. 「隐藏」按钮点击走 `hide_sticker_cmd`、「显示」走 `wake_sticker_cmd`（用 mock invoke 断言命令名）。
- **视觉验证**：`pnpm tauri dev` 启动后截图（≤30s 等待），用 `claude-vision-skill` 核对：无 edit 徽章、interact 蒙版仅三按钮、关闭→隐藏→主控台「显示」→恢复窗口。

## 5. 影响面与不做什么

- 影响文件：`ConsoleView.vue`、`StickerWindow.vue`、`lib.rs`（+ 新增 `ConsoleView.test.ts`）。
- 不做：不改 `display_mode` 的存储/类型（本次仅移除其展示标签）；不改自动收起、编辑逻辑；不新增事件名。

## 6. 完成标准

1. 主控台卡片无 "edit" 徽章；2. interact 蒙版仅 ▽✎⚙；3. 关闭便签 → 主控台立即变「显示」→ 点击「显示」窗口恢复且 Rust 日志有 `[cmd] wake_sticker` 记录；4. `pnpm test`、`cargo test`、`cargo clippy` 全绿；5. 中文 git 提交。
