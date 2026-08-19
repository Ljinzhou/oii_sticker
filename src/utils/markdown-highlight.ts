// Markdown 编辑模式的高亮（highlight.js + markdown 语法标记着色）。
// - 代码围栏（```lang ... ```）整块交给 hljs 高亮（一次调用，token 跨行正确，
//   性能远优于逐行调用）；
// - 围栏外的 markdown 语法标记（# 标题 / - [ ] 任务 / - 列表 / > 引用 /
//   **粗体** / *斜体* / `代码` / [链接](url) / $公式$ / 分隔线）按主流编辑器
//   风格着色（仅标记符号着色，内容保持默认色）。
// 返回可直接 v-html 的片段。

import hljs from "highlight.js";

function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

/** 大文本保护：超过该长度不做 hljs 高亮（仅转义），避免输入卡顿。 */
const MAX_HIGHLIGHT_LEN = 64 * 1024;

/** 围栏块整块高亮（一次 hljs 调用）。 */
function highlightFenceBlock(block: string, lang: string): string {
  if (lang && hljs.getLanguage(lang)) {
    try {
      return hljs.highlight(block, { language: lang, ignoreIllegals: true }).value;
    } catch {
      /* 高亮失败退回转义 */
    }
  }
  return escapeHtml(block);
}

/** 单行 markdown 语法标记着色（在 escape 之后的文本上执行）。
 *  行内规则用 \u0001 占位符隔离（code/链接/公式/粗体先占位，斜体最后替换，
 *  避免 **x** 被斜体规则破坏、`a*b` 被 em 破坏等交叉匹配）。 */
function markdownSyntaxHighlight(escapedLine: string): string {
  let s = escapedLine;
  // 行首结构标记
  s = s.replace(/^(#{1,6})(\s)/, '<span class="md-head">$1</span>$2');
  // 任务标记：- [ ] / - [x]
  s = s.replace(/^(\s*[-*+]\s)(\[[ xX]\])(\s)/, '$1<span class="md-task">$2</span>$3');
  // 无序列表标记
  s = s.replace(/^(\s*)([-*+])(\s)/, '$1<span class="md-list">$2</span>$3');
  // 有序列表标记（支持复合编号：1. / 1.1 / 1.1.1 / 1.1.1.）
  s = s.replace(/^(\s*)(\d+\.(?:\d+\.)*\d*)(\s)/, '$1<span class="md-list">$2</span>$3');
  // 引用标记（注意：行已 escape，> 呈现为 &gt;）
  s = s.replace(/^(\s*)(&gt;)(\s?)/, '$1<span class="md-quote">$2</span>$3');
  // 分隔线
  s = s.replace(/^(\s*)((?:-{3,}|\*{3,}|_{3,}))\s*$/, '$1<span class="md-hr">$2</span>');
  // Todo 功能块（仅高亮受控标签，HTML 不会进入渲染器）
  s = s.replace(/(&lt;\/?todo-block(?:\s+id=&quot;[^&]*&quot;)?\s*&gt;)/g, '<span class="md-fn">$1</span>');
  s = s.replace(/(&lt;\/?show-done\s*&gt;)/g, '<span class="md-fn">$1</span>');
  // 行内元素（占位 → 替换 → 还原）
  s = s.replace(/`([^`\n]+)`/g, "\u0001C$1\u0001C"); // 行内代码
  s = s.replace(/\[([^\]\n]*)\]\(([^)\n]*)\)/g, "\u0001L$1\u0002$2\u0001L"); // 链接
  s = s.replace(/\$([^$\n]+)\$/g, "\u0001M$1\u0001M"); // 行内公式
  s = s.replace(/\*\*\*([^*\n]+)\*\*\*/g, "\u0001B$1\u0001B"); // 粗斜体
  s = s.replace(/\*\*([^*\n]+)\*\*/g, "\u0001S$1\u0001S"); // 粗体
  s = s.replace(/\*([^*\n]+)\*/g, '<span class="md-em">*</span>$1<span class="md-em">*</span>');
  s = s.replace(/\u0001S([^\u0001]+)\u0001S/g, '<span class="md-strong">**</span>$1<span class="md-strong">**</span>');
  s = s.replace(/\u0001B([^\u0001]+)\u0001B/g, '<span class="md-strong">***</span>$1<span class="md-strong">***</span>');
  s = s.replace(/\u0001C([^\u0001]+)\u0001C/g, '<span class="md-code">`$1`</span>');
  s = s.replace(/\u0001L([^\u0002]+)\u0002([^\u0001]+)\u0001L/g, '<span class="md-link">[</span>$1<span class="md-link">](</span>$2<span class="md-link">)</span>');
  s = s.replace(/\u0001M([^\u0001]+)\u0001M/g, '<span class="md-math">$</span>$1<span class="md-math">$</span>');
  return s;
}

/**
 * 高亮 markdown 文本：代码围栏整块 hljs 高亮，其余行做语法标记着色。
 * 返回可直接 v-html 的片段（同步执行，普通文本 <5ms）。
 */
export function highlightMarkdown(text: string): string {
  const lines = text.split("\n");
  const heavy = text.length > MAX_HIGHLIGHT_LEN;
  const parts: string[] = [];
  let inFence = false;
  let fenceLang = "";
  let fenceStart = 0;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const fence = /^\s*```\s*([\w+-]*)\s*$/.exec(line);
    if (fence) {
      if (!inFence) {
        inFence = true;
        fenceLang = fence[1];
        fenceStart = i;
        parts.push(escapeHtml(line)); // 围栏开启行
      } else {
        // 围栏闭合：整块交给 hljs（一次调用）
        const block = lines.slice(fenceStart + 1, i).join("\n");
        parts.push(heavy ? escapeHtml(block) : highlightFenceBlock(block, fenceLang));
        parts.push(escapeHtml(line));
        inFence = false;
        fenceLang = "";
      }
    } else if (inFence) {
      // 围栏内内容行：收集到闭合时统一处理
    } else {
      parts.push(heavy ? escapeHtml(line) : markdownSyntaxHighlight(escapeHtml(line)));
    }
  }
  // 未闭合围栏：剩余行整块处理
  if (inFence) {
    const block = lines.slice(fenceStart + 1).join("\n");
    parts.push(heavy ? escapeHtml(block) : highlightFenceBlock(block, fenceLang));
  }
  return parts.join("\n");
}
