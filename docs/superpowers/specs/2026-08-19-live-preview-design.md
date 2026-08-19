# oii_sticker Live Preview 设计

## 目标

将编辑模式中的“及时预览”实现为接近 Obsidian Live Preview 的 Markdown 编辑器：Markdown 原文保留在 CodeMirror 文档中，非当前编辑元素显示渲染结果，光标进入元素时只恢复该元素源码。保存始终写入 Markdown 原文，不通过 HTML 反向转换。

## 当前问题

现有项目已经使用 CodeMirror 6 和 decoration，但当前实现存在以下风险：

- 需要验证 `editor_mode=live` 路由后 decoration 插件确实安装并产生 DOM widget。
- 行内和块级范围收集、光标语义、重叠 decoration 混在一个模块中，继续扩展会产生覆盖问题。
- 公式依赖异步 MathJax 初始化，初始化完成后必须主动刷新 decoration。
- 任务 checkbox 不能依赖当前 selection 推断行号，必须保存准确文档范围。
- 自动转换、撤销重做、表格导航、代码块和公式块尚未形成独立可测试边界。

## 设计原则

1. Markdown 文档是唯一数据源，渲染只存在于 decoration/widget 层。
2. 所有输入变换使用 CodeMirror transaction，保证撤销、重做、选区和光标位置可控。
3. 解析范围收集保持纯函数，DOM widget 只负责显示和交互。
4. 光标语义按元素精确判断，不因为光标在同一行就让整行恢复源码。
5. MathJax、列表、任务、表格等功能必须有独立回归测试。
6. 保留当前项目的 Tauri 单 runtime、camelCase invoke、无系统通知和无动画约束。

## 组件边界

- `StickerEditor.vue`：根据设置选择 Markdown 编辑器或 Live Preview。
- `StickerEditorLive.vue`：持有 CM6 实例，处理 props 同步、防抖 draft、flush 和保存事件。
- `live/LiveEditorView.ts`：创建 EditorState、主题、基础 keymap、转换 keymap 和 decoration 插件。
- `live/liveDecorations.ts`：从 Lezer 和补充扫描结果生成行内/块级范围，并按光标语义构建 decoration。
- `live/liveWidgets.ts`：公式、任务、列表标记、分隔线、代码块和表格 widget。
- `live/liveTransforms.ts`：空格、回车、Tab、Shift+Tab、退格、自动配对和快捷键 transaction。
- `live/liveTables.ts`：表格行列解析、单元格范围和 Tab 导航。
- `utils/markdown-editable.ts`：Markdown-it 片段渲染、MathJax SVG 输出和公式源码保真。

## 渲染与光标协议

### 行内元素

支持粗体、斜体、删除线、行内代码、链接和 `$...$` 公式。元素未被光标或选区触及时，整个源码范围替换为渲染 widget；光标进入范围或选区与范围相交时，移除该元素的 replace decoration，恢复源码。嵌套元素采用外层优先策略，避免重叠 replace decoration。

### 块级元素

支持标题、无序列表、有序列表、复合编号、引用、分隔线和任务清单。块标记在光标所在块显示源码，光标离开后隐藏标记并显示样式。列表编号继承正文颜色；复合编号保持既有层级语义。

### 异步公式

`mathVersion` 或等价的 CM6 state effect 作为渲染失效信号。MathJax 初始化前不替换输入文本；初始化完成后重建当前 viewport 的公式 decoration，不能要求用户保存或切换模式。

## 输入行为

- `# `、`- ` `、`1. `、`> `、`- [ ] ` 等块标记在输入触发后即时获得对应渲染。
- 列表和引用回车续接同类型标记，空项回车退出。
- Tab/Shift+Tab 调整列表层级；代码块内保持缩进。
- 空列表项行首退格退出列表。
- 标记字符支持自动配对，选区输入标记字符时自动包裹。
- 变换通过 CodeMirror transaction 完成并保留历史记录。

## 代码块、公式块和表格

- 代码块内部保持源码可编辑，离开后显示高亮结果；代码块内部不解析其他 Markdown。
- `$$...$$` 公式块使用同一 MathJax 实例，异步就绪后刷新；光标进入时恢复 TeX 源码。
- Markdown 表格保持文本源，非当前单元格显示渲染表格，当前单元格显示源码；Tab/Shift+Tab 导航，最后一个单元格 Tab 自动追加行。

## 交互 Widget

任务 checkbox 保存准确文档范围，点击只改对应 `[ ]`/`[x]`，不依赖当前 selection。链接保持键盘焦点和点击行为。工具条和斜杠菜单在核心渲染稳定后接入，不引入 Notion 块模型。

## 测试和验收

每个阶段遵循 Red-Green-Refactor：先写会失败的测试，再实现，再运行定向测试和全量测试。最终必须通过：

```powershell
.\node_modules\.bin\vitest.cmd run
.\node_modules\.bin\vue-tsc.cmd --noEmit
.\node_modules\.bin\vite.cmd build
```

UI 验收必须运行 Tauri 窗口并截图，确认截图捕获的是程序窗口；随后使用项目指定的 `claude-vision-skill` 判断 Live Preview、光标展开、公式、列表、任务和表格行为。

