// markdown-it 封装：渲染 + todo 源行映射
import MarkdownIt from "markdown-it";

const md = new MarkdownIt({
  html: false,
  linkify: true,
  breaks: false,
});

// 自写任务清单渲染：检测 `- [ ]` / `- [x]` / `- [X]` 标记，
// 输出带 data-line（源行号）的 checkbox；点击由组件事件委托处理。
const defaultListItemOpen = md.renderer.rules.list_item_open;
md.renderer.rules.list_item_open = function (tokens, idx, options, env, self) {
  const html = defaultListItemOpen
    ? defaultListItemOpen(tokens, idx, options, env, self)
    : self.renderToken(tokens, idx, options);

  const inline = tokens[idx + 1];
  if (!inline || inline.type !== "inline" || !inline.children?.length) {
    return html;
  }
  const first = inline.children[0];
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
