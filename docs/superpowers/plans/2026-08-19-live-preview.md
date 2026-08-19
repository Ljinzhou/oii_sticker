# Live Preview Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 `StickerEditorLive` 完整实现为保留 Markdown 原文、支持光标语义和即时渲染的 Obsidian Live Preview 编辑器。

**Architecture:** CodeMirror 6 保留 Markdown 文本；Lezer 语法树和补充扫描负责产生纯范围；独立的 decoration、widget、transaction transform 模块负责显示和交互。保存只提交 Markdown 原文，MathJax 使用异步失效信号刷新公式。

**Tech Stack:** Vue 3 Composition API、CodeMirror 6、`@codemirror/lang-markdown`、Lezer、Markdown-it、MathJax SVG、Vitest、Vite、Tauri 2。

---

## 全局约束

- 保留用户当前未提交改动，先审查再编辑，不使用 reset/checkout 覆盖。
- 只使用 `tauri::async_runtime`；不新增 Rust runtime。
- 不引入 `slint`、`pulldown-cmark`、`tray-icon`、`notify-rust` 直调。
- Markdown 模式行为不变；Live Preview 只改变编辑表面。
- 所有修改按小阶段使用中文 commit。
- 每个行为先写失败测试并确认红灯。
- 所有 UI 变更完成后运行实际程序截图验证，并按项目指南调用 `claude-vision-skill`。

## Task 1: 基线与渲染可见性

**Files:**
- Modify: `src/components/sticker/StickerEditor.test.ts`
- Modify: `src/components/sticker/live/LiveEditorView.test.ts`
- Modify: `src/components/sticker/live/liveDecorations.test.ts`
- Modify: `src/components/sticker/StickerEditorLive.vue`
- Modify: `src/components/sticker/live/LiveEditorView.ts`

- [ ] 写失败测试：`editor_mode=live` 的真实组件挂载后存在 `.cm-editor`、标题 decoration/widget 和渲染类名。
- [ ] 写失败测试：标题、粗体、列表、公式在光标位于其他普通文本时产生 replace decoration。
- [ ] 运行对应 Vitest，确认失败原因是 DOM/decoration 缺失而不是测试配置错误。
- [ ] 修复 LiveEditorView 的扩展安装和组件初始化时机，确保插件在 `EditorState.create` 中启用。
- [ ] 确保首次渲染不因焦点自动落在标题行而让全文显示源码；初始化 selection 放在文档末尾或使用精确元素语义。
- [ ] 运行定向测试，确认红转绿。
- [ ] 提交：`修复即时预览基础渲染链路`。

## Task 2: 行内渲染与光标穿越

**Files:**
- Create: `src/components/sticker/live/liveWidgets.ts`
- Modify: `src/components/sticker/live/liveDecorations.ts`
- Modify: `src/components/sticker/live/liveDecorations.test.ts`
- Modify: `src/components/sticker/StickerEditorLive.vue`

- [ ] 写失败测试覆盖粗体、斜体、删除线、行内代码、链接、公式、嵌套元素和未闭合标记。
- [ ] 写失败测试覆盖光标在元素外、元素内、边界、选区跨越元素四种情况。
- [ ] 将 widget DOM 创建、HTML 注入和事件忽略逻辑移到 `liveWidgets.ts`。
- [ ] 让 decoration 构建只接受范围和渲染器，不直接承担所有 widget 实现。
- [ ] 接入 `mathVersion`/state effect，使 MathJax 就绪后重建公式 widget。
- [ ] 运行定向和全量 Markdown/Live Preview 测试。
- [ ] 提交：`完善即时预览行内渲染与光标行为`。

## Task 3: 块级渲染与列表细节

**Files:**
- Modify: `src/components/sticker/live/liveDecorations.ts`
- Modify: `src/components/sticker/StickerEditorLive.vue`
- Modify: `src/components/sticker/live/LiveEditorView.ts`
- Modify: `src/components/sticker/live/liveDecorations.test.ts`

- [ ] 写失败测试覆盖标题、无序列表、有序列表、复合编号、引用和分隔线。
- [ ] 写失败测试确认编号继承正文颜色、编号与 bullet 左对齐、嵌套编号稳定。
- [ ] 实现块级 mark/replace decoration 的优先级和范围冲突规则。
- [ ] 修复光标在普通文本时不应让同一行所有 Markdown 元素恢复源码的问题。
- [ ] 保持当前已有复合编号语义和展示模式一致。
- [ ] 运行定向测试和全量测试。
- [ ] 提交：`完善即时预览块级渲染`。

## Task 4: Markdown 自动转换与操作细节

**Files:**
- Create: `src/components/sticker/live/liveTransforms.ts`
- Create: `src/components/sticker/live/liveTransforms.test.ts`
- Modify: `src/components/sticker/live/LiveEditorView.ts`
- Modify: `src/components/sticker/live/LiveEditorView.test.ts`

- [ ] 写失败测试覆盖块标记输入、列表/引用回车续接、空项退出、Tab/Shift+Tab、退格退出。
- [ ] 写失败测试确认每个变换使用单个可撤销 transaction，撤销能恢复变换前文本和选区。
- [ ] 实现纯函数 transaction builder，输入文档/选区，输出 changes、selection 和 userEvent。
- [ ] 接入 `keymap`、`transactionFilter` 或 `inputHandler`，避免重复处理同一按键。
- [ ] 实现自动配对和选区包裹，处理转义、代码块和公式上下文。
- [ ] 运行定向测试、撤销重做测试和全量测试。
- [ ] 提交：`实现即时预览 Markdown 自动转换`。

## Task 5: 代码块、公式块和任务清单

**Files:**
- Create: `src/components/sticker/live/liveBlocks.ts`
- Create: `src/components/sticker/live/liveBlocks.test.ts`
- Modify: `src/components/sticker/live/liveWidgets.ts`
- Modify: `src/components/sticker/live/liveDecorations.ts`
- Modify: `src/components/sticker/live/liveDecorations.test.ts`

- [ ] 写失败测试覆盖代码围栏、语言标记、代码块内禁用 Markdown 解析、公式块异步渲染。
- [ ] 写失败测试确认任务 checkbox 点击修改准确源范围，不依赖当前 selection。
- [ ] 实现代码块和公式块范围识别及光标进入后的源码恢复。
- [ ] 实现任务 widget 保存文档范围和稳定的点击 dispatch。
- [ ] 复用 MathJax SVG 输出和 CSS 收集逻辑，确保首次加载无需保存即可显示。
- [ ] 运行定向测试和全量测试。
- [ ] 提交：`完善代码公式与任务块编辑`。

## Task 6: Markdown 表格交互

**Files:**
- Create: `src/components/sticker/live/liveTables.ts`
- Create: `src/components/sticker/live/liveTables.test.ts`
- Modify: `src/components/sticker/live/liveWidgets.ts`
- Modify: `src/components/sticker/live/liveTransforms.ts`
- Modify: `src/components/sticker/live/LiveEditorView.ts`

- [ ] 写失败测试覆盖表头、分隔行、数据行、对齐标记和非法表格降级。
- [ ] 写失败测试覆盖光标单元格源码、Tab/Shift+Tab 导航、末单元格 Tab 新增行。
- [ ] 实现表格范围解析和只读渲染 widget，当前单元格保留源码。
- [ ] 实现表格导航 transaction，保持行列边界和撤销历史。
- [ ] 运行表格定向测试和全量测试。
- [ ] 提交：`实现即时预览表格交互`。

## Task 7: 快捷键、斜杠命令和工具条

**Files:**
- Create: `src/components/sticker/live/liveKeymaps.ts`
- Create: `src/components/sticker/live/liveKeymaps.test.ts`
- Modify: `src/components/sticker/live/LiveEditorView.ts`
- Modify: `src/components/sticker/StickerEditorLive.vue`
- Modify: `src/components/sticker/SlashMenu.vue`
- Modify: `src/components/sticker/live/liveWidgets.ts`

- [ ] 写失败测试覆盖粗体、斜体、删除线、代码、链接、标题、引用、列表、代码块、公式块和保存快捷键。
- [ ] 将快捷键实现为可组合的 CM6 keymap，避免覆盖基础编辑命令。
- [ ] 将现有 slash 命令接入当前 selection 和 transaction builder。
- [ ] 添加选中文本工具条，工具条按钮带可访问名称和 tooltip。
- [ ] 处理失焦、选区变化、保存和窗口销毁时的清理。
- [ ] 运行定向测试和全量测试。
- [ ] 提交：`完善即时预览快捷键与命令操作`。

## Task 8: 性能、可访问性与 UI 验收

**Files:**
- Modify: `src/components/sticker/live/liveDecorations.ts`
- Modify: `src/components/sticker/live/liveWidgets.ts`
- Modify: `src/components/sticker/StickerEditorLive.vue`
- Modify: `src/components/sticker/live/LiveEditorView.ts`
- Modify: relevant test files

- [ ] 写失败测试覆盖空文档、大文档、外部 modelValue 同步、编辑器销毁和 MathJax 异步刷新。
- [ ] 限制 decoration 计算到 viewport，避免输入时全量 DOM 重建。
- [ ] 缓存不变公式和 Markdown 片段渲染结果，避免重复 MathJax 计算。
- [ ] 验证 checkbox、链接、工具条键盘焦点和 tooltip。
- [ ] 清理 timer、ResizeObserver、EditorView、MathJax listener 和 widget 事件。
- [ ] 运行 `vitest`、`vue-tsc` 和 `vite build`。
- [ ] 清理残留 Vite/Tauri 进程和端口，启动实际程序验证 Markdown 模式与 Live Preview 模式。
- [ ] 截图并使用 `claude-vision-skill` 判断：标题、列表、粗体、公式、任务、表格、光标展开和保存回写。
- [ ] 提交：`完成即时预览性能与交互验收`。

## 最终交付检查

- [ ] 所有阶段中文 commit 均存在。
- [ ] `git diff --check` 无错误。
- [ ] `vitest` 全部通过。
- [ ] `vue-tsc --noEmit` 通过。
- [ ] `vite build` 通过。
- [ ] UI 截图捕获实际程序窗口并完成视觉检查。
- [ ] 工作区只保留本计划相关的已提交变更和用户原有变更，不删除用户文件。
