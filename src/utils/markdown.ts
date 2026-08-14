// markdown-it 封装：渲染 + todo 源行映射 + 编辑模式（WYSIWYG 回写）
import MarkdownIt from "markdown-it";
import TurndownService from "turndown";

const md = new MarkdownIt({
  html: false,
  linkify: true,
  breaks: false,
});

/** 编辑模式实例：任务标记保持为 `[ ]` 文本（不生成 checkbox），便于 contenteditable 编辑与回写。 */
const mdEditable = new MarkdownIt({
  html: false,
  linkify: true,
  breaks: false,
});

const turndown = new TurndownService({
  headingStyle: "atx",
  bulletListMarker: "-",
  codeBlockStyle: "fenced",
  emDelimiter: "*",
  strongDelimiter: "**",
});

// 自写任务清单渲染：检测 `- [ ]` / `- [x]` / `- [X]` 标记，
// 输出带 data-line（源行号）的 checkbox；点击由组件事件委托处理。
const defaultListItemOpen = md.renderer.rules.list_item_open;
md.renderer.rules.list_item_open = function (tokens, idx, options, env, self) {
  const html = defaultListItemOpen
    ? defaultListItemOpen(tokens, idx, options, env, self)
    : self.renderToken(tokens, idx, options);

  // 向后找 inline token（list_item 内可能夹 paragraph_open 等）
  const inline = tokens
    .slice(idx + 1)
    .find((t) => t.type === "inline" && t.children?.length);
  if (!inline) {
    return html;
  }
  const first = inline.children![0];
  if (first.type !== "text") {
    return html;
  }
  const m = /^(\[[ xX]\])\s*/.exec(first.content);
  if (!m) {
    return html;
  }
  // 剥离任务标记文本（由 checkbox 视觉替代）
  first.content = first.content.slice(m[0].length);
  const line = tokens[idx].map ? tokens[idx].map[0] : -1;
  const checked = m[1] === "[x]" || m[1] === "[X]";
  const checkbox = `<input type="checkbox" class="task-checkbox" data-line="${line}"${
    checked ? " checked" : ""
  }>`;
  return html.replace("<li", `<li class="task-item"`) + checkbox;
};

/** 渲染 markdown → HTML（含 todo checkbox 的 data-line）。 */
export function renderMarkdown(content: string): string {
  return md.render(content);
}

/** 编辑模式渲染：任务标记保留为 `[ ]`/`[x]` 纯文本（供 contenteditable 直接编辑）。 */
export function renderMarkdownEditable(content: string): string {
  const defaultListItemOpen = mdEditable.renderer.rules.list_item_open;
  mdEditable.renderer.rules.list_item_open = function (tokens, idx, options, env, self) {
    const html = defaultListItemOpen
      ? defaultListItemOpen(tokens, idx, options, env, self)
      : self.renderToken(tokens, idx, options);
    // 编辑模式不做 checkbox 转换，任务标记文本原样保留
    return html;
  };
  return mdEditable.render(content);
}

/** 编辑后的 HTML → Markdown（WYSIWYG 保存回写）。 */
export function htmlToMarkdown(html: string): string {
  return turndown.turndown(html);
}

/** 从 CSS 变量生成 rgba（前端偏好面板用）。 */
export function hexToRgba(hex: string, alpha: number): string {
  const m = /^#?([0-9a-fA-F]{6})$/.exec(hex.trim());
  if (!m) return `rgba(255, 255, 255, ${alpha})`;
  const n = parseInt(m[1], 16);
  const r = (n >> 16) & 0xff;
  const g = (n >> 8) & 0xff;
  const b = n & 0xff;
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}
