// markdown-it 封装：渲染 + todo 源行映射 + 数学公式（mathjax 离线 SVG）
import MarkdownIt from "markdown-it";
import { ref } from "vue";
import { createMathjaxInstance, mathjax } from "@mdit/plugin-mathjax";
import type { TodoBlock } from "../types";
import type { BlockUiState } from "./block-ui";
import { todoHighlightState } from "./todo-dates";

// ═══════════════ SVG 动态字体预加载 ═══════════════
// mdit-mathjax 的 asyncLoad 用动态 import(e) 加载字体模块（如
// '@mathjax/mathjax-newcm-font/js/svg/dynamic/latin.js'），vite 无法静态分析
// 该变量路径，运行时浏览器解析裸说明符失败 → 公式渲染中断。
// 修复：静态导入全部 SVG 动态字体（副作用 dynamicSetup 注册字形数据），
// 并在 mdit 初始化完成后覆盖 mathjax 单例的 asyncLoad 直接返回已解析。
// 与 mdit 插件解析到同一文件（mjs/mathjax.js 单例），此处经 "./*" 通配直通路径
import { mathjax as mathjaxCore } from "@mathjax/src/mjs/mathjax.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/accents.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/accents-b-i.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/arabic.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/arrows.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/braille.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/braille-d.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/calligraphic.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/cherokee.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/cyrillic.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/cyrillic-ss.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/devanagari.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/double-struck.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/fraktur.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/greek.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/greek-ss.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/hebrew.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/latin.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/latin-b.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/latin-bi.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/latin-i.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/marrows.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/math.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/monospace.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/monospace-ex.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/monospace-l.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/mshapes.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/phonetics.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/phonetics-ss.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/PUA.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/sans-serif.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/sans-serif-b.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/sans-serif-bi.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/sans-serif-ex.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/sans-serif-i.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/sans-serif-r.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/script.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/shapes.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/symbols.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/symbols-b-i.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/variants.js";

// ═══════════════ 数学公式（@mdit/plugin-mathjax，离线 SVG 输出） ═══════════════
// createMathjaxInstance 为异步（sync 入口是 Node 专用，浏览器构建必须用异步版）。
// 初始化完成前 $..$ 按普通文本输出，就绪后 mathVersion+1 触发视图重渲染。
type MathjaxInstance = Awaited<ReturnType<typeof createMathjaxInstance>>;

let mathInst: MathjaxInstance | null = null;
/** mathjax 就绪版本号：就绪后 +1，渲染视图据此重渲染公式。 */
export const mathVersion = ref(0);

/** 转义 HTML 属性（data-tex 内嵌 tex 源）。 */
export function escapeAttr(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/"/g, "&quot;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

// markdown-it 渲染规则的宽松签名（其官方类型带 Env，与自定义包裹互操作不便）
// eslint-disable-next-line @typescript-eslint/no-explicit-any
type RendererRule = (tokens: any[], idx: number, options: any, env: any, self: any) => string;

/**
 * 包裹 math 渲染规则：外层加 span/div（class=math-inline|math-block、
 * contenteditable=false、data-tex 保留 tex 源），供及时预览编辑与 turndown 回写。
 */
export function wrapMathRule(
  rule: RendererRule | undefined,
  className: string,
  tag: string,
) {
  return function (tokens: any[], idx: number, options: any, env: any, self: any): string {
    const inner = rule
      ? rule(tokens, idx, options, env, self)
      : self.renderToken(tokens, idx, options);
    const tex = tokens[idx].content;
    return `<${tag} class="${className}" contenteditable="false" data-tex="${escapeAttr(tex)}">${inner}</${tag}>`;
  };
}

// ── 渲染实例（StickerViewer / 展示 / 交互模式） ──
const md = new MarkdownIt({
  html: false,
  linkify: true,
  breaks: false,
});

function escapeText(value: string): string {
  return value.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

function todoBlockRule(state: any, startLine: number, _endLine: number, silent: boolean) {
  const line = state.getLines(startLine, startLine + 1, state.blkIndent, false).trim();
  const match = /^<todo-block\s+id=["']([^"']+)["']\s*><\/todo-block>\s*$/.exec(line);
  if (!match) return false;
  if (!silent) {
    const token = state.push("todo_block", "div", 0);
    token.block = true;
    token.meta = { id: match[1] };
    token.map = [startLine, startLine + 1];
  }
  state.line = startLine + 1;
  return true;
}

function showDoneRule(state: any, startLine: number, _endLine: number, silent: boolean) {
  const line = state.getLines(startLine, startLine + 1, state.blkIndent, false).trim();
  if (line !== "<show-done></show-done>") return false;
  if (!silent) {
    const token = state.push("show_done", "div", 0);
    token.block = true;
  }
  state.line = startLine + 1;
  return true;
}

md.block.ruler.before("paragraph", "todo_block", todoBlockRule, { alt: ["paragraph", "reference"] });
md.block.ruler.before("paragraph", "show_done", showDoneRule, { alt: ["paragraph", "reference"] });
md.renderer.rules.todo_block = (tokens, idx, _options, env) => {
  const blocks = Array.isArray(env?.todoBlocks) ? env.todoBlocks as TodoBlock[] : [];
  const id = (tokens[idx].meta as { id?: string } | null)?.id ?? "";
  // 每个 <todo-block> 标记渲染自己的独立卡片（根任务 + 其子任务）：
  // 一个便签可挂任意多个块，互不合并。
  return renderTodoCard(blocks, id, Boolean(env?.interactive), env?.ui as BlockUiState | undefined);
};
md.renderer.rules.show_done = (_tokens, _idx, _options, env) => {
  const blocks = Array.isArray(env?.todoBlocks) ? env.todoBlocks as TodoBlock[] : [];
  const uiKey = typeof env?.uiKey === "string" ? env.uiKey : "";
  return renderDoneCard(blocks, env?.ui as BlockUiState | undefined, uiKey);
};

/**
 * 渲染一个 <todo-block> 标记对应的任务卡片。
 *
 * 三层结构（块 → 父任务 → 子任务）：
 *   - 标记对应的 root 行是**块**，只作为容器提供卡头标题，**不作为任务条目显示**；
 *   - depth 0 = 父任务（parent_id = 块.id），depth 1 = 子任务（parent_id = 父任务.id）；
 *   - 有子任务的父任务左侧出现折叠按钮，子任务可收起/展开；
 *   - 块内没有任何任务时显示「暂无任务」；
 *   - 计数只统计任务（父 + 子），不含块本身。
 *
 * 折叠/子任务显隐完全由 BlockUiState（用户操作持久化）驱动，程序不自动改变。
 */
function renderTodoCard(blocks: TodoBlock[], id: string, interactive: boolean, ui?: BlockUiState): string {
  const root = blocks.find((block) => block.id === id);
  if (!root) return `<div class="todo-block-card todo-block-missing" data-todo-id="${escapeText(id)}">未找到任务</div>`;

  const childrenOf = new Map<string, TodoBlock[]>();
  for (const block of blocks) {
    if (block.sticker_id !== root.sticker_id || !block.parent_id) continue;
    const list = childrenOf.get(block.parent_id) ?? [];
    list.push(block);
    childrenOf.set(block.parent_id, list);
  }

  // 从「块」往下展开：depth 0 = 父任务，depth 1 = 子任务。块自身不入列。
  type Entry = { block: TodoBlock; depth: number; ancestors: string[]; hasKids: boolean };
  const entries: Entry[] = [];
  const walk = (parentId: string, depth: number, ancestors: string[]) => {
    for (const block of childrenOf.get(parentId) ?? []) {
      const kids = childrenOf.get(block.id) ?? [];
      entries.push({ block, depth, ancestors, hasKids: kids.length > 0 });
      walk(block.id, depth + 1, [...ancestors, block.id]);
    }
  };
  walk(root.id, 0, []);

  const cardFolded = ui?.folds?.[root.id] === true;
  const subHidden = (taskId: string) => ui?.subs?.[taskId] === true;
  // 可见性：卡片整体折叠由 ul hidden 控制；条目自身再被任一祖先的「隐藏子任务」命中则不渲染。
  const isVisible = (entry: Entry) => !entry.ancestors.some((pid) => subHidden(pid));

  // 提醒高亮只看父任务（子任务不设提醒/截止）
  const parentEntries = entries.filter((entry) => entry.depth === 0);
  const anyReminded = parentEntries.some((entry) => todoHighlightState(entry.block).reminded && !entry.block.is_completed);
  const anyOverdue = parentEntries.some((entry) => todoHighlightState(entry.block).overdue && !entry.block.is_completed);

  const done = entries.filter((entry) => entry.block.is_completed).length;

  // 空块：块内一条任务都没有
  const items = entries.length === 0
    ? '<li class="tb-empty">暂无任务</li>'
    : entries.map((entry) => {
        if (!isVisible(entry)) return "";
        const hl = todoHighlightState(entry.block);
        const cls = [
          entry.block.is_completed ? "tb-done" : "",
          entry.depth > 0 ? "tb-sub" : "",
          hl.reminded ? "tb-reminded" : "",
          hl.overdue ? "tb-overdue" : "",
        ].filter(Boolean).join(" ");
        // 折叠按钮：只有「有子任务的父任务」可点；
        // 其余行用同宽占位保持缩进对齐（不显示箭头）。
        const caret = entry.hasKids
          ? `<span class="tb-caret" data-caret="${escapeText(entry.block.id)}" title="显示/隐藏子任务">${subHidden(entry.block.id) ? "▸" : "▾"}</span>`
          : '<span class="tb-caret tb-caret-placeholder" aria-hidden="true"></span>';
        return `<li class="${cls}">${caret}<input type="checkbox" class="todo-task-checkbox" data-todo-id="${escapeText(entry.block.id)}"${entry.block.is_completed ? " checked" : ""}${interactive ? "" : " disabled"}><span class="tb-name">${escapeText(entry.block.title || "未命名任务")}</span></li>`;
      }).join("");

  // 卡头使用独立块标题（block_title）；卡头下拉箭头折叠/展开整个列表。
  // 注意：block_title 为空时应回退到"未命名任务"，而不是冒用块自身的 title。
  const head = root.block_title?.trim() || "未命名任务";
  const flags = `${anyReminded ? '<span class="tb-flag tb-flag-reminded"> 提醒中</span>' : ""}${anyOverdue ? '<span class="tb-flag tb-flag-overdue">已逾期</span>' : ""}`;
  const cardCls = `todo-block-card${anyReminded ? " todo-block-reminded" : ""}${anyOverdue ? " todo-block-overdue" : ""}`;
  const foldCaret = `<span class="tb-caret tb-fold-caret" data-fold="${escapeText(root.id)}" title="折叠/展开任务块">${cardFolded ? "▸" : "▾"}</span>`;
  return `<div class="${cardCls}" data-todo-id="${escapeText(root.id)}"><div class="tb-head">${foldCaret}<span class="tb-title">${escapeText(head)}</span>${flags}<span class="tb-count">${done} / ${entries.length}</span></div><ul class="tb-list"${cardFolded ? " hidden" : ""}>${items}</ul></div>`;
}

/**
 * 渲染 <show-done> 已完成任务卡：
 *  - 下拉框选择数据来源（全部 / 某个任务块），选择持久化到 BlockUiState；
 *  - 每项任务后显示完成时刻（completed_at，本地时间）；
 *  - 卡头下拉箭头折叠/展开列表，状态同样持久化。
 */
function renderDoneCard(blocks: TodoBlock[], ui?: BlockUiState, uiKey = ""): string {
  const srcId = uiKey ? (ui?.doneSrc?.[uiKey] ?? "") : (ui?.doneSrc?.[""] ?? "");
  const roots = blocks.filter((block) => !block.parent_id);

  // 来源范围：选中某根任务时 = 该根 + 其整棵子树；否则全部任务。
  let scope: Set<string> | null = null;
  if (srcId && roots.some((block) => block.id === srcId)) {
    scope = new Set([srcId]);
    let grew = true;
    while (grew) {
      grew = false;
      for (const block of blocks) {
        if (block.parent_id && scope.has(block.parent_id) && !scope.has(block.id)) {
          scope.add(block.id);
          grew = true;
        }
      }
    }
  }

  const done = blocks.filter((block) => block.is_completed && (!scope || scope.has(block.id)));
  const options = [
    `<option value=""${srcId === "" ? " selected" : ""}>全部任务</option>`,
    ...roots.map((block) =>
      `<option value="${escapeText(block.id)}"${block.id === srcId ? " selected" : ""}>${escapeText(block.block_title?.trim() || block.title || "未命名任务")}</option>`),
  ];
  const folded = uiKey ? (ui?.doneFolds?.[uiKey] === true) : (ui?.doneFolds?.[""] === true);
  const caret = `<span class="tb-caret" data-donefold="${escapeText(uiKey)}" title="折叠/展开已完成列表">${folded ? "▸" : "▾"}</span>`;
  const select = `<select class="sd-source" data-sd-key="${escapeText(uiKey)}" title="选择要显示哪个任务块的已完成任务">${options.join("")}</select>`;
  const items = done.map((block) =>
    `<li><input type="checkbox" class="todo-task-checkbox" checked disabled><span class="tb-name tb-done">${escapeText(block.title || "未命名任务")}</span><span class="db-time">${escapeText(block.completed_at ?? "")}</span></li>`,
  ).join("");
  return `<div class="done-block-card"><div class="db-head">${caret}<span class="db-title">已完成 ${done.length}</span>${select}</div><ul class="db-list"${folded ? " hidden" : ""}>${items}</ul></div>`;
}

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

/** 复合编号行（编辑层标记）→ 标准嵌套列表语法（渲染视图专用，不改源码）：
 *  `1.1 文本` → `  1. 文本`（嵌套深度 = 链长-1），供 markdown-it 识别为嵌套列表，
 *  编号层级由 CSS counter（counters(item, ".")）显示为 1. / 1.1 / 1.1.1。 */
export function normalizeCompoundLists(md: string): string {
  let inFence = false;
  let fenceChar = "";
  return md
    .split("\n")
    .map((line) => {
      const t = line.trimStart();
      // 围栏开关跟踪：``` / ~~~
      if (!inFence && (t.startsWith("```") || t.startsWith("~~~"))) {
        inFence = true;
        fenceChar = t.startsWith("```") ? "`" : "~";
        return line;
      }
      if (inFence && ((fenceChar === "`" && t.startsWith("```")) || (fenceChar === "~" && t.startsWith("~~~")))) {
        inFence = false;
        return line;
      }
      if (inFence) return line; // 代码块内不转换
      const m = /^(\s*)(\d+(?:\.\d+)+)\.?(\s.*)$/.exec(line);
      if (!m) return line;
      const depth = m[2].split(".").length - 1;
      return `${m[1]}${"  ".repeat(depth)}1.${m[3]}`;
    })
    .join("\n");
}

/** 渲染 markdown → HTML（含 todo checkbox 的 data-line 与 mathjax SVG；
 *  复合编号先归一化为嵌套列表语法）。 */
export function renderMarkdown(
  content: string,
  todoBlocks: TodoBlock[] = [],
  interactive = false,
  ui?: BlockUiState,
  uiKey?: string,
): string {
  return md.render(normalizeCompoundLists(content), { todoBlocks, interactive, ui, uiKey });
}

/** 收集 mathjax 渲染产生的 SVG CSS 并清空缓存（渲染后调用，注入全局 style）。 */
export async function collectMathStyle(): Promise<string> {
  if (!mathInst) return "";
  const css = await mathInst.outputStyle();
  mathInst.clearStyle();
  return css;
}

/** 异步初始化 mathjax：完成后注册进 md，并 bump mathVersion 触发重渲染。
 *  解析为实例（或 null 表示失败），供编辑实例复用注册。 */
export const mathInstancePromise = createMathjaxInstance({
  output: "svg",
  delimiters: "dollars",
})
  .then((inst) => {
    if (!inst) {
      console.error("[math] mathjax 实例创建失败");
      return null;
    }
    mathInst = inst;
    md.use(mathjax, inst);
    // 字体已全部静态预加载（dynamicSetup 已注册字形数据），
    // 覆盖 mdit 设置的 asyncLoad（动态 import 在 vite 打包下会失败）
    mathjaxCore.asyncLoad = (name: string) => {
      console.debug(`[math] 字体已预加载，跳过动态加载：${name}`);
      return Promise.resolve({});
    };
    // 公式规则包裹（须在插件注册后覆盖，保留 data-tex）
    md.renderer.rules.math_inline = wrapMathRule(
      md.renderer.rules.math_inline,
      "math-inline",
      "span",
    );
    md.renderer.rules.math_block = wrapMathRule(
      md.renderer.rules.math_block,
      "math-block",
      "div",
    );
    mathVersion.value++;
    return inst;
  })
  .catch((e) => {
    console.error("[math] mathjax 初始化失败：", e);
    return null;
  });

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
