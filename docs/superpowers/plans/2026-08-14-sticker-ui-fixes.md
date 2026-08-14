# 便签 UI 三项修复 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 移除主控台 "edit" 模式徽章、移除便签交互模式关闭按钮、修复便签关闭后主控台「隐藏/显示」按钮不同步且「显示」无响应的问题。

**Architecture:** 前端 Vue 组件改动（ConsoleView/StickerWindow）+ Rust 命令层改动（hide/wake 两个 command）。便签窗口「关闭」语义从销毁改为隐藏：`onClosed` 走 `hide_sticker_cmd`；Rust 隐藏后广播 `push-update`，主控台监听时同步刷新 `openIds`；`wake_sticker_cmd` 增加「窗口不存在时经 run_on_main_thread 重建」防御层，并清除 display 模式锁定的 min/max 尺寸（与钩子唤醒路径一致）。

**Tech Stack:** Vue 3 `<script setup>` + TypeScript、Pinia、Vitest + @vue/test-utils（jsdom）、Tauri 2（Rust）。测试 mock 模式沿用 `vitest.config.ts`（jsdom，include `src/**/*.test.ts`），组件测试参照 `src/components/markdown/MarkdownView.test.ts`。

**测试脚手架说明（两个新测试文件共用的 mock 方式）：** 用 `vi.hoisted` 创建 `invokeMock` / `listenMock`（捕获事件处理器），`vi.mock("../../composables/useTauri")` 与 `vi.mock("@tauri-apps/api/window")`；stores 也经 useTauri 导入，同一 mock 生效。挂载时 `global.plugins: [createPinia()]`，异步用 `flushPromises()`。

---

### Task 1: 主控台移除模式徽章（ConsoleView.vue）

**Files:**
- Create: `src/components/console/ConsoleView.test.ts`
- Modify: `src/components/console/ConsoleView.vue:67-70`（删 reminderText）、`:118`（删 span）、`:276-282`（删 .mode-badge 样式）

- [ ] **Step 1: 写失败测试（脚手架 + 第一条测试）**

Create `src/components/console/ConsoleView.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import { createPinia } from "pinia";
import ConsoleView from "./ConsoleView.vue";

// —— mock Tauri IPC 层：捕获 invoke 调用与 listen 注册 ——
const mocks = vi.hoisted(() => {
  const handlers = new Map<string, (payload: unknown) => void>();
  return {
    handlers,
    invokeMock: vi.fn(),
    listenMock: vi.fn(async (event: string, handler: (p: unknown) => void) => {
      handlers.set(event, handler);
      return () => {};
    }),
  };
});

vi.mock("../../composables/useTauri", () => ({
  invoke: (...args: unknown[]) => mocks.invokeMock(...args),
  listen: (e: string, h: (p: unknown) => void) => mocks.listenMock(e, h),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ label: "main", minimize: vi.fn(), close: vi.fn() }),
}));

const sticker = {
  id: 1,
  parent_id: null,
  title: "欢迎使用 oii_sticker",
  content: "# 标题",
  heading_level: 0,
  pos_x: 200, pos_y: 140, width: 400, height: 500,
  opacity: 0.9,
  bg_color: null,
  always_on_top: false,
  auto_scroll: false,
  is_completed: false,
  alert_active: false,
  display_mode: "edit",
  created_at: "",
  updated_at: "",
};

function setupInvoke() {
  mocks.invokeMock.mockImplementation((cmd: string) => {
    switch (cmd) {
      case "list_stickers_cmd": return Promise.resolve([sticker]);
      case "list_open_sticker_ids_cmd": return Promise.resolve([1]);
      case "get_config_cmd": return Promise.resolve({ entries: {} });
      default: return Promise.resolve(undefined);
    }
  });
}

async function mountConsole() {
  const wrapper = mount(ConsoleView, { global: { plugins: [createPinia()] } });
  await flushPromises();
  return wrapper;
}

beforeEach(() => {
  mocks.handlers.clear();
  mocks.invokeMock.mockReset();
  mocks.listenMock.mockClear();
  setupInvoke();
});

describe("ConsoleView", () => {
  it("卡片不渲染模式徽章（无 display_mode 文本标签）", async () => {
    const wrapper = await mountConsole();
    expect(wrapper.find(".mode-badge").exists()).toBe(false);
    expect(wrapper.text()).not.toContain("edit");
  });
});
```

- [ ] **Step 2: 运行测试确认失败**

Run: `pnpm vitest run src/components/console/ConsoleView.test.ts`
Expected: FAIL —— `.mode-badge` 存在、文本含 "edit"（当前实现渲染徽章）

- [ ] **Step 3: 实现（删除徽章）**

Modify `src/components/console/ConsoleView.vue`:
- 删除函数（:67-70）：
```ts
function reminderText(s: Sticker): string {
  // 提醒信息由 attrs 提供；列表简化显示模式即可
  return s.display_mode;
}
```
- 删除模板行（:118）：
```html
<span class="mode-badge">{{ reminderText(s) }}</span>
```
- 删除样式块（:276-282）：
```css
.mode-badge {
  font-size: 11px;
  color: #4f7cff;
  background: rgba(79, 124, 255, 0.1);
  border-radius: 6px;
  padding: 2px 8px;
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `pnpm vitest run src/components/console/ConsoleView.test.ts`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add -A && git commit -m "refactor: 移除主控台卡片模式徽章（display_mode 标签）"
```

---

### Task 2: push-update 到达时同步刷新 openIds（ConsoleView.vue）

**Files:**
- Modify: `src/components/console/ConsoleView.test.ts`（追加测试）
- Modify: `src/components/console/ConsoleView.vue:86`

- [ ] **Step 1: 写失败测试**

在 `ConsoleView.test.ts` 的 describe 内追加：

```ts
  it("收到 push-update 事件后同时刷新列表与窗口打开状态", async () => {
    await mountConsole();
    mocks.invokeMock.mockClear();
    const handler = mocks.handlers.get("sticky://push-update");
    expect(handler).toBeTruthy();
    handler!(1);
    await flushPromises();
    const cmds = mocks.invokeMock.mock.calls.map((c) => c[0]);
    expect(cmds).toContain("list_stickers_cmd");
    expect(cmds).toContain("list_open_sticker_ids_cmd");
  });
```

- [ ] **Step 2: 运行测试确认失败**

Run: `pnpm vitest run src/components/console/ConsoleView.test.ts`
Expected: FAIL —— push-update 后只调了 `list_stickers_cmd`，无 `list_open_sticker_ids_cmd`

- [ ] **Step 3: 实现**

Modify `src/components/console/ConsoleView.vue:86`，由：
```ts
  unlisteners.push(await listen("sticky://push-update", () => notes.refresh()));
```
改为：
```ts
  unlisteners.push(
    await listen("sticky://push-update", () => {
      notes.refresh();
      refreshOpenIds();
    }),
  );
```

- [ ] **Step 4: 运行测试确认通过**

Run: `pnpm vitest run src/components/console/ConsoleView.test.ts`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add -A && git commit -m "fix: 主控台收到 push-update 时同步刷新窗口打开状态（隐藏/显示按钮）"
```

---

### Task 3: interact 蒙版移除关闭按钮（StickerWindow.vue）

**Files:**
- Create: `src/components/sticker/StickerWindow.test.ts`
- Modify: `src/components/sticker/StickerWindow.vue:198`（删 ✕ 按钮）、`:297-300`（删 .ov-btn.close 样式）

- [ ] **Step 1: 写失败测试（脚手架 + 第一条测试）**

Create `src/components/sticker/StickerWindow.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from "vitest";
import { shallowMount, flushPromises } from "@vue/test-utils";
import { createPinia } from "pinia";
import StickerWindow from "./StickerWindow.vue";
import type { StickerMode } from "../../types";

const mocks = vi.hoisted(() => {
  const handlers = new Map<string, (payload: unknown) => void>();
  return {
    handlers,
    invokeMock: vi.fn(),
    listenMock: vi.fn(async (event: string, handler: (p: unknown) => void) => {
      handlers.set(event, handler);
      return () => {};
    }),
  };
});

vi.mock("../../composables/useTauri", () => ({
  invoke: (...args: unknown[]) => mocks.invokeMock(...args),
  listen: (e: string, h: (p: unknown) => void) => mocks.listenMock(e, h),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ label: "sticker-7" }),
}));

const sticker = {
  id: 7,
  parent_id: null,
  title: "便签",
  content: "# 便签",
  heading_level: 0,
  pos_x: 200, pos_y: 140, width: 400, height: 500,
  opacity: 0.9,
  bg_color: null,
  always_on_top: false,
  auto_scroll: false,
  is_completed: false,
  alert_active: false,
  display_mode: "interact",
  created_at: "",
  updated_at: "",
};

const effectivePrefs = {
  opacity: 0.9,
  title_centered: false,
  title_font_size: 14,
  body_font_size: 13,
  bg_color: "#FFF4D6",
  text_color: "#222222",
  auto_scroll_speed: 30,
};

function setupInvoke(mode: StickerMode) {
  mocks.invokeMock.mockImplementation((cmd: string) => {
    switch (cmd) {
      case "get_sticker_cmd":
        return Promise.resolve({ ...sticker, display_mode: mode });
      case "effective_prefs_cmd":
        return Promise.resolve(effectivePrefs);
      default:
        return Promise.resolve(undefined);
    }
  });
}

async function mountSticker(mode: StickerMode) {
  const wrapper = shallowMount(StickerWindow, { global: { plugins: [createPinia()] } });
  await flushPromises();
  return wrapper;
}

beforeEach(() => {
  mocks.handlers.clear();
  mocks.invokeMock.mockReset();
  mocks.listenMock.mockClear();
});

describe("StickerWindow", () => {
  it("交互模式蒙版仅三个功能按钮（无关闭按钮）", async () => {
    setupInvoke("interact");
    const wrapper = await mountSticker("interact");
    const btns = wrapper.findAll(".ov-btn");
    expect(btns.length).toBe(3);
    expect(wrapper.find(".ov-btn.close").exists()).toBe(false);
  });
});
```

- [ ] **Step 2: 运行测试确认失败**

Run: `pnpm vitest run src/components/sticker/StickerWindow.test.ts`
Expected: FAIL —— `.ov-btn` 有 4 个、存在 `.ov-btn.close`

- [ ] **Step 3: 实现**

Modify `src/components/sticker/StickerWindow.vue`:
- 删除行（:198）：
```html
      <button class="ov-btn close" title="关闭窗口" @click.stop="onClosed">✕</button>
```
- 删除样式（:297-300）：
```css
.ov-btn.close:hover {
  background: #ffe3e3;
  color: #d33;
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `pnpm vitest run src/components/sticker/StickerWindow.test.ts`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add -A && git commit -m "feat: 便签交互模式移除关闭窗口按钮（蒙版仅收起/编辑/设置）"
```

---

### Task 4: 便签关闭改为隐藏（onClosed → hide_sticker_cmd）

**Files:**
- Modify: `src/components/sticker/StickerWindow.test.ts`（追加测试）
- Modify: `src/components/sticker/StickerWindow.vue:141-144`

- [ ] **Step 1: 写失败测试**

在 `StickerWindow.test.ts` 的 describe 内追加：

```ts
  it("编辑模式点击关闭按钮调用 hide_sticker_cmd（隐藏而非销毁窗口）", async () => {
    setupInvoke("edit");
    const wrapper = await mountSticker("edit");
    mocks.invokeMock.mockClear();
    wrapper.findComponent({ name: "StickerEditor" }).vm.$emit("closed");
    await flushPromises();
    expect(mocks.invokeMock).toHaveBeenCalledWith("hide_sticker_cmd", { id: 7 });
  });
```

- [ ] **Step 2: 运行测试确认失败**

Run: `pnpm vitest run src/components/sticker/StickerWindow.test.ts`
Expected: FAIL —— 当前 onClosed 调 `getCurrentWindow().close()`，未调用 `hide_sticker_cmd`

- [ ] **Step 3: 实现**

Modify `src/components/sticker/StickerWindow.vue:141-144`，由：
```ts
async function onClosed() {
  // 只关闭窗口，不删除数据（主控台保留该便签）
  await getCurrentWindow().close();
}
```
改为：
```ts
async function onClosed() {
  // 关闭=隐藏窗口（不删除数据）；由 Rust hide_sticker_cmd 隐藏并广播
  // push-update，主控台收到后把按钮切到"显示"，点"显示"再经 wake 恢复。
  await invoke("hide_sticker_cmd", { id: stickerId });
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `pnpm vitest run src/components/sticker/StickerWindow.test.ts`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add -A && git commit -m "fix: 便签关闭改为隐藏窗口（走 hide_sticker_cmd，不再销毁窗口）"
```

---

### Task 5: Rust——hide 广播 push-update、wake 重建窗口 + 清尺寸锁

**Files:**
- Modify: `src-tauri/src/lib.rs:402-413`（wake_sticker_cmd）、`:430-438`（hide_sticker_cmd）

- [ ] **Step 1: 修改 hide_sticker_cmd（隐藏成功后广播）**

Modify `src-tauri/src/lib.rs`，由：
```rust
/// 隐藏便签窗口（数据保留，主控台显示"显示"按钮）。
#[tauri::command]
fn hide_sticker_cmd(app: tauri::AppHandle, id: i64) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(&format!("sticker-{id}")) {
        win.hide().map_err(|e| format!("隐藏窗口失败: {e}"))?;
        tracing::info!("[cmd] hide_sticker id={id}");
    }
    Ok(())
}
```
改为：
```rust
/// 隐藏便签窗口（数据保留，主控台显示"显示"按钮）。
#[tauri::command]
fn hide_sticker_cmd(app: tauri::AppHandle, id: i64) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(&format!("sticker-{id}")) {
        win.hide().map_err(|e| format!("隐藏窗口失败: {e}"))?;
        tracing::info!("[cmd] hide_sticker id={id}");
        // 广播 push-update：主控台收到后刷新 openIds，把按钮切到"显示"
        events::emit_push_update(&app, id);
    } else {
        tracing::warn!("[cmd] hide_sticker id={id} 窗口不存在");
    }
    Ok(())
}
```

- [ ] **Step 2: 修改 wake_sticker_cmd（重建防御 + 清尺寸锁）**

Modify `src-tauri/src/lib.rs`，由：
```rust
/// 唤醒便签窗口：置前聚焦 + 可 resize（display 收起后使用）。
#[tauri::command]
fn wake_sticker_cmd(app: tauri::AppHandle, id: i64) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(&format!("sticker-{id}")) {
        win.set_ignore_cursor_events(false)
            .map_err(|e| format!("取消穿透失败: {e}"))?;
        win.set_resizable(true).map_err(|e| format!("设置 resize 失败: {e}"))?;
        let _ = win.show();
        let _ = win.set_focus();
        tracing::info!("[cmd] wake_sticker id={id}");
    }
    Ok(())
}
```
改为：
```rust
/// 唤醒便签窗口：置前聚焦 + 可 resize（display 收起后使用）。
/// 防御：窗口不存在（曾被销毁）时按数据库记录经主线程重建，保证主控台
/// "显示"按钮始终有效；同步清除 display 模式锁定的 min/max 尺寸
/// （与鼠标钩子唤醒路径一致，否则 set_resizable(true) 不生效）。
#[tauri::command]
fn wake_sticker_cmd(app: tauri::AppHandle, id: i64) -> Result<(), String> {
    let label = format!("sticker-{id}");
    let win = if let Some(win) = app.get_webview_window(&label) {
        win
    } else {
        let state = app.state::<AppState>();
        let s = state
            .with_conn(|c| commands::get_sticker(c, id))
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("便签 #{id} 不存在，无法唤醒"))?;
        // 已知坑：IPC 线程同步建窗会卡死，必须投递主线程（AI-DEV-GUIDE §2.2）
        let (tx, rx) = std::sync::mpsc::channel();
        let app2 = app.clone();
        app.run_on_main_thread(move || {
            let result = create_sticker_win(
                &app2, s.id, &s.title, s.pos_x, s.pos_y, s.width, s.height,
            )
            .map_err(|e| format!("重建便签窗口失败: {e}"));
            let _ = tx.send(result);
        })
        .map_err(|e| format!("投递主线程失败: {e}"))?;
        let rebuilt = rx
            .recv()
            .map_err(|e| format!("等待重建窗口失败: {e}"))??;
        tracing::info!("[cmd] wake_sticker id={id} 窗口不存在，已按数据库记录重建");
        rebuilt
    };
    win.set_ignore_cursor_events(false)
        .map_err(|e| format!("取消穿透失败: {e}"))?;
    win.set_resizable(true).map_err(|e| format!("设置 resize 失败: {e}"))?;
    win.set_min_size(None::<tauri::Size>)
        .map_err(|e| format!("清除最小尺寸失败: {e}"))?;
    win.set_max_size(None::<tauri::Size>)
        .map_err(|e| format!("清除最大尺寸失败: {e}"))?;
    let _ = win.show();
    let _ = win.set_focus();
    tracing::info!("[cmd] wake_sticker id={id}");
    Ok(())
}
```

- [ ] **Step 3: Rust 编译与测试验证**

Run（工作目录 `E:\AAACodeProjectMy\oii_sticker\src-tauri`）:
```powershell
cargo check
cargo clippy
cargo test
```
Expected: check 通过、clippy 0 警告、test 99 通过（本改动无单测，不新增用例）

- [ ] **Step 4: 提交**

```bash
git add -A && git commit -m "fix: 隐藏便签广播状态同步事件；唤醒命令支持重建已销毁窗口并清除尺寸锁"
```

---

### Task 6: 全量验证

- [ ] **Step 1: 前端全量测试 + 构建**

Run（工作目录 `E:\AAACodeProjectMy\oii_sticker`）:
```powershell
pnpm test
pnpm build
```
Expected: vitest 全部通过（原 9 + 新 4 = 13）；vue-tsc + vite build 通过

- [ ] **Step 2: Rust 全量复核**

Run: `cargo test`、`cargo clippy`（同 Task 5）
Expected: 99 通过、0 警告

- [ ] **Step 3: 运行时视觉验证（见 spec §4）**

`pnpm tauri dev` 启动（先清残留进程与 1420 端口，AI-DEV-GUIDE §2.2）→ 截图（≤30s 等待；未捕获窗口则直接告知用户）→ `claude-vision-skill` 核对：
1. 主控台卡片无 "edit" 徽章；2. interact 蒙版仅 ▽✎⚙ 三按钮；3. 便签 ✕（编辑工具条）→ 主控台变「显示」→ 点「显示」窗口恢复，Rust 日志出现 `[cmd] hide_sticker` / `[cmd] wake_sticker`。

- [ ] **Step 4: 完成提交（如视觉验证引发调整）

```bash
git add -A && git commit -m "docs: 便签UI修复视觉验证记录"
```

---

## Self-Review 备注

- Spec 覆盖：徽章移除=Task 1；交互蒙版 ✕=Task 3；关闭同步+wake=Task 2/4/5；测试=Task 1-4；视觉验证=Task 6 ✓
- 类型一致性：测试中 `sticker` 字段与 `types.ts Sticker` 一致；`StickerMode` 复用现有类型 ✓
- 超出 spec 的补充（已注明）：wake 同步清除 min/max 尺寸锁（修复后端分析 M1，与钩子唤醒路径对齐）
