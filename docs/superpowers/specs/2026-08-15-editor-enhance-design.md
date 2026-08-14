# 编辑模式增强设计（即时预览 / 语法高亮 / 数学公式）

日期：2026-08-15
状态：已批准（用户确认三决策 + 模块独立文件要求）

## 背景

oii_sticker 编辑模式当前为 Markdown 原生文本 textarea（左上角保存/取消）。本轮增强：

1. overlay 编辑态与内容间距过窄/重叠
2. 移除取消按钮；✎ 编辑按钮进入编辑后变蓝，再点一次自动保存并退出
3. 原取消按钮位置改为「即时预览 | Markdown」分段开关（全局配置）
4. 代码块语法高亮（highlight.js）
5. 数学公式渲染（@mdit/plugin-mathjax，离线 SVG 输出）

## 用户决策（已确认）

- 开关偏好：**全局配置**（system_config，key `editor_mode`，`"markdown" | "live"`，默认 `"markdown"`）
- 即时预览实现：**Typora 式**（contenteditable 渲染视图直接编辑，保存转回 Markdown 原文）
- ✎ 退出编辑：**自动保存后退出**
- 模块要求：**每个功能模块独立 vue 文件**，不堆进单一文件

## 架构与组件拆分

```
src/utils/
  markdown.ts                 渲染实例 md（mathjax + data-tex 规则 + checkbox）+ collectMathStyle
  markdown-editable.ts        编辑实例 mdEditable（保留 [ ] 文本）+ htmlToMarkdown（turndown + math 回写）
  markdown-highlight.ts       highlightMarkdown 行级高亮（围栏按语言，围栏外转义）
src/components/sticker/
  StickerWindow.vue           窗口容器：overlay（✎ toggle/蓝、开关、间距）、模式路由（已有，改）
  StickerEditor.vue           编辑容器：持有 draft，按 editor_mode 路由子组件，保存逻辑（改薄）
  StickerEditorMarkdown.vue   Markdown 模式：textarea + 行号 gutter + 高亮层（新）
  StickerEditorLive.vue       即时预览模式：contenteditable + 渲染 + turndown 回写（新）
src/components/markdown/
  MarkdownView.vue            渲染视图：渲染后 hljs.highlightElement + math CSS 注入（改）
src/main.ts                   hljs github 浅色主题全局引入
src/components/console/SettingsPanel.vue   便签默认组新增"编辑模式默认形态"（改）
```

## 关键实现点

### 1. overlay 间距
编辑模式下 `.body` 顶部 padding 增至 48px（class 绑定 `editing`），与按钮条彻底分离；交互模式不动。

### 2. ✎ toggle + 自动保存退出
- ✎ 点击：`mode === 'edit' ? exitEdit() : applyMode('edit')`
- `exitEdit()`：调 `editorRef.save()`（失败则留在编辑态）→ `applyMode('interact')`
- 编辑模式 ✎ 加 `.ov-btn.active`（蓝底白字）
- 移除「取消」按钮

### 3. 开关
- 分段开关「即时预览 | Markdown」置于 overlay 左侧（保存按钮旁）
- 读写 `settings.set/get("editor_mode")`，切换即时生效（draft 经 v-model 保持不丢）

### 4. Markdown 编辑模式（textarea + 高亮层）
- 结构：`[gutter 行号] [pre.hl-layer（v-html 高亮，pointer-events:none）] [textarea（文字透明、caret 可见）]`
- 高亮层与 textarea 同字体/字号/行高/padding，scrollTop 同步；textarea 颜色 `transparent` + `caret-color: #333`
- 行级高亮：围栏（```lang）内按 lang 逐行 hljs.highlight；围栏外 escapeHtml；无语言不高亮
- 行号开关沿用 `editor_line_numbers`（默认关）

### 5. 即时预览模式（contenteditable）
- 渲染用 `renderMarkdownEditable`（保留 `[ ]` 文本，避免 checkbox 与编辑冲突）
- `@input` → `htmlToMarkdown(innerHTML)` → `update:modelValue`（150ms 防抖）
- turndown 注册 math 规则：`span.math-inline[data-tex]` → `$..$`，`div.math-block[data-tex]` → `$$..$$`
- 公式容器 `contenteditable="false"`（整体对象，避免 SVG 被破坏）

### 6. mathjax
- `createMathjaxInstance({ output: "svg", delimiters: "dollars" })`（sync 导入），md/mdEditable 共用
- 自定义 `math_inline`/`math_block` 渲染规则：外包 `span.math-inline|div.math-block` + `data-tex`（转义）
- 渲染后 `collectMathStyle()` 收集 SVG CSS → 注入全局 `<style id="mathjax-style">`（MarkdownView 与 Live 编辑器各自注入）

### 7. highlight.js（渲染视图）
- `import "highlight.js/styles/github.css"`（main.ts 全局）
- MarkdownView 在 html 更新后 `nextTick` → `pre code` 逐个 `hljs.highlightElement`（仅未高亮过的）
- 即时预览模式不跑 hljs（避免 contenteditable DOM 被替换）

## 测试

- `markdown-highlight.test.ts`：围栏行高亮 / 围栏外转义 / 无语言不高亮
- `markdown-editable.test.ts`：math 回写（$..$、$$..$$ 保真）、checkbox 文本保留
- `StickerEditor.test.ts`：更新为容器路由断言（默认 Markdown 子组件渲染 textarea）
- 全量：vitest + vue-tsc + vite build；Rust 无改动（cargo test 仅回归）

## 依赖

- `highlight.js`、`@mdit/plugin-mathjax`、`turndown`、`@types/turndown`
