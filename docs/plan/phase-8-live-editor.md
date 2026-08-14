# Phase 8：复刻主流 Markdown 编辑器（即时预览编辑器重构）

> 调研依据：docs/research/editor-behavior.md（Notion/Obsidian/Typora 多 agent 调研汇总）
> 目标：即时预览模式从 contenteditable 自研方案重构为 CodeMirror 6 内核，
> 完整复刻 Typora/Obsidian 的操作方式（行内渲染、光标穿越、自动转换、块编辑、快捷键等）。

## 0. 背景与问题

当前即时预览（`StickerEditorLive.vue`）为 contenteditable + turndown 回写：
- 输入 `**x**` 不能立即渲染（需防抖重渲染 + 光标恢复，体验割裂）
- 无法实现光标穿越语法标记、元素展开编辑、自动转换、表格导航等主流编辑器核心细节
- 用户明确要求"完全按照 notion/obsidian/typora 操作方式复刻，所有 markdown 标签可渲染"

**技术选型**：CodeMirror 6（Obsidian Live Preview 同款内核）
- `@codemirror/lang-markdown` 语法高亮 + markdown-it 渲染 decoration（行内/块级渲染）
- 内置多光标、折叠、行号、选区、历史
- 表格/代码块/公式用 decoration 渲染 + 专门 keymap 处理导航

## 1. 依赖

```
@codemirror/state  @codemirror/view  @codemirror/language  @codemirror/lang-markdown
@codemirror/commands  @codemirror/autocomplete  @lezer/highlight
（markdown-it / highlight.js / @mdit/plugin-mathjax 沿用现有）
```

## 2. 架构

```
src/components/sticker/
  StickerEditor.vue          编辑容器（保持：draft + editor_mode 路由 + save）
  StickerEditorMarkdown.vue  保持现状（textarea + 高亮层）
  StickerEditorLive.vue      重构：CM6 编辑器宿主（EditorView 生命周期、draft 双向同步）
  live/
    LiveEditorView.ts        CM6 实例创建/配置/销毁（新）
    liveDecorations.ts       markdown-it 渲染 → Decoration（行内/块级）+ 光标穿越（新）
    liveKeymap.ts            回车/Tab/退格/快捷键/自动转换行为（新）
    liveWidgets.ts           代码块/公式块/表格/checkbox 渲染小部件（新）
    liveToolbar.ts           浮动工具条（选中文本弹出）（新）
```

## 3. 分阶段执行

### Phase A：CM6 基础集成（可运行、可保存）
- [ ] 安装依赖，建立 `LiveEditorView.ts`（EditorView + basicSetup + markdown 语言）
- [ ] draft ↔ CM6 双向同步（外部更新 setDoc；编辑 updateListener 回写；防抖保存回写沿用 flush 语义）
- [ ] 行号、Tab 缩进（代码块内）、Ctrl+S 保存、焦点管理
- [ ] 保留当前样式观感（透明背景/字体/字号 edit_font_size）
- **验证**：vitest（CM6 实例化 + 双向同步单测）+ 构建 + 中文提交

### Phase B：行内标记渲染与光标穿越（Live Preview 核心）
- [ ] `liveDecorations.ts`：markdown-it 解析 → 行内 token（em/strong/code/del/link/math）→
      Decoration.replace（渲染样式，标记隐藏，保留原文可编辑性）
- [ ] 光标穿越：光标在渲染 span 内时显示源码（widget 切换）；选择跨越时标记临时显示
- [ ] 输入闭合标记即时渲染（`**x**` 输入完即变粗体，无防抖）
- [ ] 任务标记 `- [ ]` 渲染 checkbox + 点击切换（写回文本 + 落库）
- **验证**：交互单测（decoration 生成/光标行为）+ 构建 + 中文提交

### Phase C：块级交互（自动转换 / 回车 / Tab / 退格 / 斜杠命令）
- [ ] 行首快捷输入：`# `、`- `、`1. `、`> `、`- [ ] `、`---`、``` ``` ```、`$$`、`|表头|`
      → 输入后自动转换（含代码围栏自动闭合、公式输入区）
- [ ] 回车行为：列表/引用续接、连续两次回车退出、标题行尾转段落、Shift+Enter 软换行
- [ ] Tab：列表嵌套/取消嵌套；代码块缩进；表格单元格导航（末格自动加行）
- [ ] 空项退格退出列表/引用；自动配对（`**`/`*`/`` ` ``/`$` 配对与包裹选中文本）
- [ ] **`/` 斜杠命令菜单**（Notion 风格）：输入 `/` 弹出块类型菜单（文本/标题1-3/无序/有序/
      任务/引用/分割线/代码块/公式块/表格/图片等，可过滤），回车插入对应块模板；
      与现有 slash 命令（src/slash，模板插入）整合：块菜单优先，原有模板命令并入
- **验证**：keymap 行为单测 + 构建 + 中文提交

### Phase D：块渲染与操作（代码块/公式块/表格/引用）
- [ ] 代码块：widget 渲染（hljs 高亮、语言选择下拉、行号可选、Tab 缩进、点击块外退出）
- [ ] 公式块：`$$` + Enter 输入区（实时预览 MathJax SVG）、↑/↓/Ctrl+Enter/点击外部完成、
      光标移回已渲染公式重新进入编辑；内联 `$...$` 渲染 + data-tex 保存保真（沿用）
- [ ] 表格：源码 ↔ 渲染 widget；Tab 导航、Ctrl+Enter 插行、右键菜单增删行列、对齐
- [ ] 引用/分隔线/图片/脚注渲染 widget
- **验证**：各 widget 渲染单测 + 构建 + 中文提交

### Phase E：快捷键 / 浮动工具条 / 细节
- [ ] 快捷键：Ctrl+B/I/U/Shift+S/E、Ctrl+1-6/0、Ctrl+K、Ctrl+Shift+K/M/T/Q、Ctrl+[ / ]、Ctrl+/
- [ ] 浮动工具条：选中文本弹出（加粗/斜体/删除线/代码/链接/公式）
- [ ] 折叠（标题/列表）、Emoji `:` 补全（可选）
- [ ] 源码 ↔ 即时预览 快速切换（Ctrl+/）、焦点/打字机模式（可选）
- [ ] 全量回归：保留 Markdown 模式（textarea+高亮层）不动，双模式并存
- **验证**：vitest 全绿 + cargo 回归 + 构建 + 中文提交 + 用户实测截图

## 4. 风险与对策

| 风险 | 对策 |
|---|---|
| CM6 decoration 与光标穿越实现复杂 | 参考 Obsidian 开源思路（markdown-it + widget decoration）；先实现行内再块级 |
| 表格/公式渲染中编辑与保存保真 | 渲染 widget 内保留源码（data-tex 模式沿用）；turndown 仅用于整块回写兜底 |
| 与现有 Markdown 模式（textarea）行为不一致 | 双模式并存；Markdown 模式保持现状，Live 模式逐步对齐 |
| mathjax 异步初始化 | 沿用 mathVersion 机制触发 decoration 重算 |

## 5. 验收标准（用户视角）

1. 输入 `**文字**` 立即变粗体，光标在粗体中间可编辑、标记可展开
2. 行首输入 `# `/`- `/`- [ ] `/`> `/`---`/``` ``` ```/`$$`/`|表头|` 自动转换
3. 列表回车续接、两次回车退出；Tab 嵌套；空项退格退出
4. 代码块/公式/表格渲染美观、可编辑（语言选择、公式实时预览、表格 Tab 导航+右键菜单）
5. 点击任务 checkbox 切换；Ctrl+B 等快捷键生效；选中文本浮动工具条
6. 输入 `/` 弹出块类型斜杠菜单，选择后插入对应块（含代码块/公式块/表格）
7. 保存后 Markdown 原文完整保真（含公式/表格/代码块）
