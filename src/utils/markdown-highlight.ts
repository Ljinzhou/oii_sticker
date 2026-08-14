// Markdown 编辑模式的行级语法高亮（highlight.js）。
// ```lang 围栏内逐行按语言高亮（token 跨行时允许碎片化，视觉可接受），
// 围栏外普通转义——用于 textarea 下层的高亮层（pre 覆盖）。
import hljs from "highlight.js";

function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

/** 行级高亮：围栏内按语言着色，围栏外转义。返回可直接 v-html 的片段。 */
export function highlightMarkdown(text: string): string {
  let inFence = false;
  let lang = "";
  return text
    .split("\n")
    .map((line) => {
      // 围栏开关行：```lang 或 ```（含缩进）
      const fence = /^\s*```\s*([\w+-]*)\s*$/.exec(line);
      if (fence) {
        inFence = !inFence;
        lang = inFence ? fence[1] : "";
        return escapeHtml(line);
      }
      // 围栏内且有明确语言：按语言逐行高亮
      if (inFence && lang && hljs.getLanguage(lang)) {
        try {
          return hljs.highlight(line, { language: lang, ignoreIllegals: true }).value;
        } catch {
          /* 高亮失败退回转义 */
        }
      }
      return escapeHtml(line);
    })
    .join("\n");
}
