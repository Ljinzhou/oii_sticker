// 编辑回写封装：即时预览（WYSIWYG）用的渲染实例 + HTML→Markdown 回写。
// 任务标记保留为 `[ ]` 纯文本（不生成 checkbox）；公式以 data-tex 保真回写。
import MarkdownIt from "markdown-it";
import TurndownService from "turndown";
import { mathInstancePromise, mathVersion, wrapMathRule } from "./markdown";
import { mathjax } from "@mdit/plugin-mathjax";

// ── 编辑实例：与渲染实例同配置，但任务标记保持文本（默认渲染器行为） ──
const mdEditable = new MarkdownIt({
  html: false,
  linkify: true,
  breaks: false,
});

// mathjax 就绪后注册进编辑实例（与渲染实例共享同一个 mathjax 实例）
mathInstancePromise.then((inst) => {
  if (!inst) return;
  mdEditable.use(mathjax, inst);
  mdEditable.renderer.rules.math_inline = wrapMathRule(
    mdEditable.renderer.rules.math_inline,
    "math-inline",
    "span",
  );
  mdEditable.renderer.rules.math_block = wrapMathRule(
    mdEditable.renderer.rules.math_block,
    "math-block",
    "div",
  );
});

/** 即时预览渲染：任务标记保留为 `[ ]`/`[x]` 纯文本（供 contenteditable 直接编辑）。 */
export function renderMarkdownEditable(content: string): string {
  void mathVersion.value; // mathjax 就绪后触发重渲染
  return mdEditable.render(content);
}

// ── HTML → Markdown 回写（turndown + math data-tex 规则） ──
const turndown = new TurndownService({
  headingStyle: "atx",
  bulletListMarker: "-",
  codeBlockStyle: "fenced",
  emDelimiter: "*",
  strongDelimiter: "**",
});

// 数学公式：span.math-inline[data-tex] → $..$，div.math-block[data-tex] → $$..$$
turndown.addRule("mathInline", {
  filter: (node) => node.classList.contains("math-inline"),
  replacement: (_content, node) => {
    const tex = node.getAttribute("data-tex") ?? "";
    return `$${tex}$`;
  },
});
turndown.addRule("mathBlock", {
  filter: (node) => node.classList.contains("math-block"),
  replacement: (_content, node) => {
    const tex = node.getAttribute("data-tex") ?? "";
    return `\n\n$$${tex}$$\n\n`;
  },
});

/** 编辑后的 HTML → Markdown（保存回写；公式经 data-tex 保真还原）。 */
export function htmlToMarkdown(html: string): string {
  return turndown.turndown(html);
}
