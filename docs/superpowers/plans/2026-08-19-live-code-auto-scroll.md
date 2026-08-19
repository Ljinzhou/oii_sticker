# 及时预览代码块与自动滚动完善实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让及时预览正确渲染 fenced 代码块，并为每个便签提供可即时生效的自动滚动速度，保证内容在上下边界间持续平滑往返。

**Architecture:** CodeMirror 继续保存 Markdown 原文，完整 `FencedCode` 节点通过块级 replace widget 显示，光标或选区进入时恢复整块源码。滚动推进抽成纯状态函数，Vue 组件用 RAF 时间戳和浮点位置驱动；速度沿用既有便签偏好字段，经 Pinia effective preference 即时传播。

**Tech Stack:** Vue 3 Composition API、Pinia、CodeMirror 6、Lezer Markdown、Markdown-it、Vitest、Vue Test Utils、Vite、Tauri 2。

---

## 执行结果

- [x] Task 1：fenced 代码块及时预览、光标/选区恢复源码、StateField block decoration。
- [x] Task 2：基于 RAF 时间戳和浮点逻辑位置的自动滚动往返状态机。
- [x] Task 3：便签级自动滚动速度设置（`5-120 px/s`，步进 `5`）和 Pinia 即时生效。
- [x] Task 4：全量测试、类型检查、生产构建和 diff 检查通过；实际 Tauri UI 启动受本机 WebView2/数据库占用阻断，详见交付说明。

验证记录：`vitest` 16 个文件、112 个测试通过；`vue-tsc --noEmit`、`vite build`、`git diff --check` 均通过。

## 全局约束

- 遵循 `docs/AI-DEV-GUIDE.md`，保留 Markdown 原文和现有编辑模式行为。
- 不修改 Rust schema：复用 `StickerPrefs.auto_scroll_speed` 与 `EffectivePrefs.auto_scroll_speed`。
- 速度范围固定为 `5-120 px/s`，步进 `5 px/s`。
- 每项生产代码之前先写测试并观察预期失败。
- 每完成一项独立功能后运行定向测试并使用中文提交。
- 最终运行全量 Vitest、`vue-tsc`、Vite build 和 `git diff --check`；UI 验收按项目视觉流程执行。

## 文件结构

- Modify: `src/components/sticker/live/liveDecorations.ts`：收集 fenced 代码块范围并创建块级 replace decoration。
- Modify: `src/components/sticker/live/liveWidgets.ts`：新增语义化 `CodeBlockWidget`。
- Modify: `src/components/sticker/live/liveDecorations.test.ts`：验证代码块范围和光标/选区行为。
- Modify: `src/components/sticker/StickerEditorLive.vue`：代码块视觉样式。
- Create: `src/utils/auto-scroll.ts`：不依赖 DOM 的往返滚动状态推进函数。
- Create: `src/utils/auto-scroll.test.ts`：滚动算法边界回归测试。
- Modify: `src/components/sticker/StickerWindow.vue`：使用 RAF 时间差、浮点位置和便签 effective speed。
- Modify: `src/components/sticker/StickerWindow.test.ts`：验证便签速度接入和 RAF 生命周期。
- Create: `src/stores/prefs.test.ts`：验证速度本地 patch。
- Modify: `src/stores/prefs.ts`：支持 `auto_scroll_speed` 即时更新。
- Create: `src/components/sticker/StickerSettings.test.ts`：验证速度控件和保存字段。
- Modify: `src/components/sticker/StickerSettings.vue`：添加速度滑块与数值显示。

### Task 1: fenced 代码块 Live Preview

**Files:**
- Modify: `src/components/sticker/live/liveDecorations.test.ts`
- Modify: `src/components/sticker/live/liveDecorations.ts`
- Modify: `src/components/sticker/live/liveWidgets.ts`
- Modify: `src/components/sticker/StickerEditorLive.vue`

- [ ] **Step 1: 写代码块失败测试**

在 `liveDecorations.test.ts` 增加完整代码块、语言标识、块外渲染、块内恢复和选区相交测试。测试通过 `collectBlockRanges(view)` 查找 `kind === "code-block"`，并通过 `buildLiveDecorations(view)`/DOM 断言 `.live-code-block pre > code.language-rust`。

```ts
const source = "```rust\nfn main() {\n  println!(\"hello\");\n}\n```";
const view = makeView(source, source.length);
expect(collectBlockRanges(view).filter((r) => r.kind === "code-block")).toHaveLength(1);
expect(document.querySelector(".live-code-block pre > code.language-rust")).not.toBeNull();
```

- [ ] **Step 2: 运行定向测试确认 RED**

Run: `./node_modules/.bin/vitest.cmd run src/components/sticker/live/liveDecorations.test.ts`

Expected: `code-block` 类型或 `.live-code-block` 尚不存在而失败。

- [ ] **Step 3: 实现范围和 Widget**

在 `BlockRange.kind` 中加入 `"code-block"` 和可选 `source`、`language`。只接受有结束 fence 的完整节点，提取去除 fence 后的代码正文。新增：

```ts
export class CodeBlockWidget extends WidgetType {
  constructor(readonly code: string, readonly language?: string) { super(); }
  eq(other: CodeBlockWidget) { return other.code === this.code && other.language === this.language; }
  toDOM() {
    const wrapper = document.createElement("div");
    const pre = document.createElement("pre");
    const code = document.createElement("code");
    wrapper.className = "live-code-block";
    if (this.language) code.className = `language-${this.language}`;
    code.textContent = this.code;
    pre.append(code);
    wrapper.append(pre);
    return wrapper;
  }
}
```

`buildLiveDecorations()` 对代码块使用 selection-range 相交语义，而不是“当前行”语义；块外时创建 `Decoration.replace({ block: true, widget })`。

- [ ] **Step 4: 加入代码块样式并验证 GREEN**

在 `StickerEditorLive.vue` 中为 `.live-code-block`、`pre`、`code` 添加稳定宽度、保留空白和横向滚动样式。运行 Task 1 定向测试，Expected: PASS。

- [ ] **Step 5: 中文提交**

```powershell
git add src/components/sticker/live/liveDecorations.ts src/components/sticker/live/liveWidgets.ts src/components/sticker/live/liveDecorations.test.ts src/components/sticker/StickerEditorLive.vue
git commit -m "修复及时预览代码块渲染"
```

### Task 2: 往返滚动状态机

**Files:**
- Create: `src/utils/auto-scroll.test.ts`
- Create: `src/utils/auto-scroll.ts`
- Modify: `src/components/sticker/StickerWindow.vue`
- Modify: `src/components/sticker/StickerWindow.test.ts`

- [ ] **Step 1: 写滚动算法失败测试**

创建纯函数期望 API：

```ts
export interface AutoScrollState { position: number; direction: 1 | -1 }
export function advanceAutoScroll(
  state: AutoScrollState,
  maxPosition: number,
  speed: number,
  deltaMs: number,
): AutoScrollState
```

测试正常向下、底部反射、顶部反射、低速小数推进、长帧多次跨界和 `maxPosition <= 0`。

- [ ] **Step 2: 运行定向测试确认 RED**

Run: `./node_modules/.bin/vitest.cmd run src/utils/auto-scroll.test.ts`

Expected: 模块不存在而失败。

- [ ] **Step 3: 实现最小反射算法**

计算 `distance = max(0, speed) * max(0, deltaMs) / 1000`，将位置按方向推进；用周期 `2 * maxPosition` 折叠任意长位移，在奇偶半周期映射为向下/向上位置，边界方向取离开边界的方向。

- [ ] **Step 4: 接入 StickerWindow RAF**

`StickerWindow.vue` 增加 `lastScrollTimestamp` 和浮点 `scrollPosition`。`tickScroll(timestamp)` 首帧只记录时间，后续调用 `advanceAutoScroll`，写入 `el.scrollTop`。速度使用：

```ts
const autoScrollSpeed = computed(
  () => prefs.effective?.auto_scroll_speed ?? settings.autoScrollSpeed,
);
```

启动、停止和卸载时重置时间戳；watch 加入 `autoScrollSpeed`，使速度变化立即重启。

- [ ] **Step 5: 运行定向测试确认 GREEN**

Run: `./node_modules/.bin/vitest.cmd run src/utils/auto-scroll.test.ts src/components/sticker/StickerWindow.test.ts`

Expected: PASS，底部后的下一有效帧位置小于最大值。

- [ ] **Step 6: 中文提交**

```powershell
git add src/utils/auto-scroll.ts src/utils/auto-scroll.test.ts src/components/sticker/StickerWindow.vue src/components/sticker/StickerWindow.test.ts
git commit -m "修复自动滚动边界往返"
```

### Task 3: 便签级滚动速度设置

**Files:**
- Create: `src/stores/prefs.test.ts`
- Modify: `src/stores/prefs.ts`
- Create: `src/components/sticker/StickerSettings.test.ts`
- Modify: `src/components/sticker/StickerSettings.vue`

- [ ] **Step 1: 写 store 与设置组件失败测试**

Store 测试将 effective speed 初始化为 `30`，调用 `applyLocal({ auto_scroll_speed: 65 })` 后期望值为 `65`。组件测试 mock Tauri 返回 speed `45`，期望存在：

```html
<input data-testid="auto-scroll-speed" type="range" min="5" max="120" step="5">
```

触发 input/change 后，断言 `update_sticker_prefs_cmd` payload 包含 `auto_scroll_speed`。

- [ ] **Step 2: 运行定向测试确认 RED**

Run: `./node_modules/.bin/vitest.cmd run src/stores/prefs.test.ts src/components/sticker/StickerSettings.test.ts`

Expected: `applyLocal` 不处理该字段且速度控件不存在。

- [ ] **Step 3: 实现 Pinia 本地 patch**

扩展 `applyLocal()` 参数和赋值：

```ts
auto_scroll_speed?: number;
if (patch.auto_scroll_speed !== undefined) e.auto_scroll_speed = patch.auto_scroll_speed;
```

- [ ] **Step 4: 实现设置控件与保存链路**

在设置组件加入 `autoScrollSpeed = ref(30)`，`load()` 从 effective prefs 读取；`applyPrefsSoon()` 与 `commitPrefs()` 均包含 `auto_scroll_speed: autoScrollSpeed.value`。自动滚动开关下一行使用滑块，显示 `${autoScrollSpeed} px/s`，控件仅在自动滚动开启时可用。

- [ ] **Step 5: 运行定向测试确认 GREEN**

Run: `./node_modules/.bin/vitest.cmd run src/stores/prefs.test.ts src/components/sticker/StickerSettings.test.ts src/components/sticker/StickerWindow.test.ts`

Expected: PASS，速度立即进入 effective prefs，持久化字段正确。

- [ ] **Step 6: 中文提交**

```powershell
git add src/stores/prefs.ts src/stores/prefs.test.ts src/components/sticker/StickerSettings.vue src/components/sticker/StickerSettings.test.ts
git commit -m "添加便签自动滚动速度设置"
```

### Task 4: 集成回归与 UI 验收

**Files:**
- Modify if needed: 本计划涉及的测试或样式文件
- Modify: `docs/superpowers/plans/2026-08-19-live-code-auto-scroll.md`（勾选执行状态）

- [ ] **Step 1: 运行全量自动测试**

```powershell
./node_modules/.bin/vitest.cmd run
./node_modules/.bin/vue-tsc.cmd --noEmit
./node_modules/.bin/vite.cmd build
git diff --check
```

Expected: 所有命令退出码 `0`。

- [ ] **Step 2: 审查完整 diff**

确认未修改 Rust schema、Markdown 保存协议或无关文件；检查 RAF、定时器和 CodeMirror widget 均有清理/稳定等价语义。

- [ ] **Step 3: 启动程序并做 UI 验收**

启动 `pnpm tauri dev`，验证 Rust fenced 代码块在光标外渲染、进入块内恢复源码；设置滑块显示 `5-120 px/s`；开启自动滚动后观察至少一次到底向上和到顶向下。

- [ ] **Step 4: 截图与视觉检查**

捕获实际程序窗口截图，调用项目指定 `claude-vision-skill` 检查代码块和速度控件布局。若网络或窗口遮挡导致无法判断，保留自动化证据并在交付中明确报告。

- [ ] **Step 5: 最终中文提交**

```powershell
git add docs/superpowers/specs/2026-08-19-live-code-auto-scroll-design.md docs/superpowers/plans/2026-08-19-live-code-auto-scroll.md
git commit -m "记录代码块与自动滚动完善方案"
```
