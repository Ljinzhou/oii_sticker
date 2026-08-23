// 及时预览的行内渲染 decoration（Live Preview 核心）。
// 机制（Obsidian Live Preview 同思路）：
// - 用 lezer 语法树（@codemirror/lang-markdown 自带）提取行内元素
//   （StrongEmphasis/Emphasis/Strikethrough/InlineCode/Link），
//   数学公式（$..$）用正则补充扫描（lezer 默认不识别 $）；
// - 光标所在行的元素**不渲染**（显示源码，便于编辑标记），其余行渲染；
// - 嵌套元素只渲染最外层（非重叠贪心）；
// - 渲染用 markdown-it 片段渲染（保证嵌套/公式正确），输出为 replace widget。
import { EditorState, Range, RangeSet, StateEffect, StateField } from "@codemirror/state";
import { Decoration, DecorationSet, EditorView, ViewPlugin } from "@codemirror/view";
import { ensureSyntaxTree, syntaxTree } from "@codemirror/language";
import { refreshLivePreview } from "./liveEffects";
import { mathVersion } from "../../../utils/markdown";
import {
  HeadingMarkWidget,
  HrWidget,
  InlineRenderWidget,
  ListMarkWidget,
  MathBlockWidget,
  QuoteMarkWidget,
  CodeBlockWidget,
  DoneBlockWidget,
  renderFragment,
  TaskCheckboxWidget,
  TodoBlockWidget,
} from "./liveWidgets";
import type { TodoBlock } from "../../../types";

interface InlineRange {
  from: number;
  to: number;
  kind: "render" | "task" | "math";
  className?: string;
  checked?: boolean;
}

/** 块级渲染范围：标题标记/列表标记/引用标记/分隔线。 */
export interface BlockRange {
  from: number;
  to: number;
  kind: "heading-mark" | "heading-line" | "listmark" | "quote" | "quote-line" | "hr" | "compound-line" | "code-block" | "math-block" | "todo-block" | "done-block";
  level?: number; // heading 级别 / 复合编号嵌套深度
  ordinal?: string; // 有序列表显示编号（如 "1."、"1.1."）；无序为 "•"
  source?: string;
  language?: string;
  todoId?: string;
}

/** 收集行内元素范围（lezer 树 + math 正则），返回非重叠列表。 */
export function collectInlineRanges(view: EditorView): InlineRange[] {
  const state = view.state;
  const doc = state.doc;
  const text = doc.toString();
  // CM6 语法树为惰性解析：强制同步解析到文档末尾，避免取到空/残缺树
  let tree = syntaxTree(state);
  if (tree.length < doc.length) {
    tree = ensureSyntaxTree(state, doc.length) ?? tree;
  }
  const ranges: InlineRange[] = [];

  // 代码块范围（math 正则需要跳过）
  const codeRanges: Array<[number, number]> = [];

  tree.iterate({
    enter: (node) => {
      const name = node.type.name;
      const from = node.from;
      const to = node.to;
      if (name === "FencedCode") {
        // 代码块整体跳过子树（内部非行内元素），范围记入 math 排除表
        codeRanges.push([from, to]);
        return false;
      }
      if (name === "InlineCode") {
        // 行内代码：是渲染目标，同时内部不参与 math 扫描
        codeRanges.push([from, to]);
        ranges.push({ from, to, kind: "render" });
        return false;
      }
      const kind =
        name === "StrongEmphasis" || name === "Emphasis" || name === "Strikethrough" || name === "Link"
          ? ("render" as const)
          : name === "TaskMarker"
            ? ("task" as const)
            : null;
      if (kind) {
        const slice = doc.sliceString(from, to);
        ranges.push(
          kind === "task"
            ? { from, to, kind, checked: /\[[xX]\]/.test(slice) }
            : {
                from,
                to,
                kind,
                className: {
                  StrongEmphasis: "live-strong",
                  Emphasis: "live-em",
                  Strikethrough: "live-del",
                  Link: "live-link",
                }[name],
              },
        );
      }
    },
  });

  // 数学公式（$..$，非转义，跳过代码块内）
  const mathRe = /(?<!\\)\$([^$\n]+)\$/g;
  let m: RegExpExecArray | null;
  while ((m = mathRe.exec(text))) {
    const from = m.index;
    const to = from + m[0].length;
    if (codeRanges.some(([cf, ct]) => from >= cf && to <= ct)) continue;
    ranges.push({ from, to, kind: "math" });
  }

  // 排序 + 非重叠贪心（外层优先；跳过与已接受重叠的嵌套内层）
  // 同起点时外层范围优先，嵌套元素随后被非重叠贪心规则跳过。
  ranges.sort((a, b) => a.from - b.from || b.to - a.to);
  const accepted: InlineRange[] = [];
  for (const r of ranges) {
    const prev = accepted[accepted.length - 1];
    if (prev && r.from < prev.to) continue;
    accepted.push(r);
  }
  return accepted;
}

/** 渲染行内范围 → Decoration。 */
function inlineDecoration(view: EditorView, r: InlineRange): Decoration {
  const text = view.state.doc.sliceString(r.from, r.to);
  if (r.kind === "task") {
    return Decoration.replace({
      widget: new TaskCheckboxWidget(r.checked ?? false, view, r.from, r.to),
    });
  }
  if (r.kind === "math") {
    return Decoration.replace({
      widget: new InlineRenderWidget(renderFragment(text), "live-math"),
    });
  }
  const cls = r.className ?? "";
  return Decoration.replace({
    widget: new InlineRenderWidget(renderFragment(text), cls),
  });
}

/** 收集块级渲染范围（标题/列表标记/引用/分隔线）。
 *  有序列表编号按嵌套层级计算（1. / 1.1. / 1.1.1.）。 */
function collectBlockRangesFromState(state: EditorState): BlockRange[] {
  const doc = state.doc;
  let tree = syntaxTree(state);
  if (tree.length < doc.length) {
    tree = ensureSyntaxTree(state, doc.length) ?? tree;
  }
  const ranges: BlockRange[] = [];
  // 代码块范围（复合编号正则扫描需跳过）
  const codeRanges: Array<[number, number]> = [];

  // ⚠ 复合编号行（编辑层标记：1.1 / 3.1.1 / 1.1.1.）优先于 lezer 识别：
  // lezer 会把 "1.1 b" 解析为普通列表项（ListMark "1."，编号按 2. 递增），
  // 与用户源码的复合编号语义冲突。故先扫描复合编号行并记录范围，
  // 遍历树时跳过与其重叠的 ListMark。
  const text = doc.toString();
  const lines = text.split("\n");
  const compoundMarks: Array<{ from: number; to: number; ordinal: string; lineStart: number; lineEnd: number; depth: number }> = [];
  let offset = 0;
  for (const line of lines) {
    const start = offset;
    const end = offset + line.length;
    const m = /^(\s*)(\d+\.\d+(?:\.\d+)*\.?)(\s)/.exec(line);
    if (m) {
      // 嵌套深度 = 段数 - 1（去掉可能的尾点再数段）
      const depth = m[2].replace(/\.$/, "").split(".").length - 1;
      const numStart = start + m[1].length;
      compoundMarks.push({
        from: numStart,
        to: numStart + m[2].length,
        ordinal: m[2],
        lineStart: start,
        lineEnd: end,
        depth,
      });
    }
    offset = end + 1; // +1 换行符
  }

  // 列表层级栈（enter/leave 维护）
  const stack: Array<{ type: "ol" | "ul"; count: number }> = [];
  // 当前 ListItem 上下文（ListMark 处理时使用）
  let currentItem: { ordered: boolean; ordinal: string } | null = null;

  tree.iterate({
    enter: (node) => {
      const name = node.type.name;
      const from = node.from;
      const to = node.to;
      if (name === "FencedCode") {
        codeRanges.push([from, to]);
        const source = doc.sliceString(from, to);
        const opening = /^[ \t]*(`{3,}|~{3,})([^\r\n]*)\r?\n/.exec(source);
        if (opening) {
          const marker = opening[1];
          const closing = new RegExp(`\\r?\\n[ \\t]*${marker[0]}{${marker.length},}[ \\t]*$`).exec(source);
          if (closing) {
            const info = opening[2].trim().split(/\s+/, 1)[0] ?? "";
            const language = /^[A-Za-z0-9_+-]+$/.test(info) ? info : undefined;
            ranges.push({
              from,
              to,
              kind: "code-block",
              source,
              language,
            });
          }
        }
        return false;
      }
      if (name === "BulletList") {
        stack.push({ type: "ul", count: 0 });
        return;
      }
      if (name === "OrderedList") {
        stack.push({ type: "ol", count: 0 });
        return;
      }
      if (name === "ListItem") {
        if (stack.length) stack[stack.length - 1].count++;
        const ordered = stack[stack.length - 1]?.type === "ol";
        const ordinal = stack
          .filter((s) => s.type === "ol")
          .map((s) => s.count)
          .join(".");
        currentItem = { ordered, ordinal };
        return;
      }
      if (name === "ListMark") {
        // 复合编号行已被正则优先识别：跳过 lezer 的 ListMark（避免编号冲突；
        // lezer 的标记可能是复合编号的前缀，如 "1." ⊂ "1.1"）
        if (compoundMarks.some((c) => c.from === from && c.to >= to)) return;
        if (currentItem) {
          ranges.push({
            from,
            to,
            kind: "listmark",
            ordinal: currentItem.ordered ? `${currentItem.ordinal}.` : "•",
          });
        }
        return;
      }
      if (name === "HeaderMark") {
        // 标题级别：行首 # 数量（setext 标题按所在行前缀推断）
        const line = doc.lineAt(from);
        const m = /^\s*(#{1,6})/.exec(line.text);
        let markTo = to;
        while (markTo < line.to && /[ \t]/.test(doc.sliceString(markTo, markTo + 1))) {
          markTo++;
        }
        ranges.push({
          from,
          to: markTo,
          kind: "heading-mark",
          level: m ? m[1].length : 1,
        });
        return;
      }
      if (name === "ATXHeading1" || name === "ATXHeading2" || name === "ATXHeading3"
        || name === "ATXHeading4" || name === "ATXHeading5" || name === "ATXHeading6") {
        const level = Number(name.replace("ATXHeading", ""));
        ranges.push({ from, to, kind: "heading-line", level });
        return;
      }
      if (name === "SetextHeading1" || name === "SetextHeading2") {
        ranges.push({
          from,
          to,
          kind: "heading-line",
          level: name === "SetextHeading1" ? 1 : 2,
        });
        return;
      }
      if (name === "QuoteMark") {
        ranges.push({ from, to, kind: "quote" });
        return;
      }
      if (name === "Blockquote") {
        ranges.push({ from, to, kind: "quote-line" });
        return;
      }
      if (name === "HorizontalRule") {
        ranges.push({ from, to, kind: "hr" });
      }
    },
    leave: (node) => {
      const name = node.type.name;
      if (name === "BulletList" || name === "OrderedList") {
        stack.pop();
      }
      if (name === "ListItem") {
        currentItem = null;
      }
    },
  });

  // 受控功能标签与 $$...$$ 不是 CodeMirror Markdown grammar 的原生节点，
  // 因而在语法树遍历后按行补充。围栏代码范围始终保持源码。
  let blockMathStart: number | null = null;
  let lineOffset = 0;
  for (const line of lines) {
    const lineStart = lineOffset;
    const lineEnd = lineStart + line.length;
    lineOffset = lineEnd + 1;
    if (codeRanges.some(([from, to]) => lineStart >= from && lineEnd <= to)) {
      blockMathStart = null;
      continue;
    }
    const trimmed = line.trim();
    const todoMatch = /^<todo-block\s+id=["']([^"']+)["']\s*><\/todo-block>$/.exec(trimmed);
    if (todoMatch) {
      ranges.push({ from: lineStart, to: lineEnd, kind: "todo-block", source: line, todoId: todoMatch[1] });
      continue;
    }
    if (trimmed === "<show-done></show-done>") {
      ranges.push({ from: lineStart, to: lineEnd, kind: "done-block", source: line });
      continue;
    }
    if (blockMathStart !== null) {
      if (trimmed.endsWith("$$")) {
        ranges.push({
          from: blockMathStart,
          to: lineEnd,
          kind: "math-block",
          source: text.slice(blockMathStart, lineEnd),
        });
        blockMathStart = null;
      }
      continue;
    }
    if (!trimmed.startsWith("$$")) continue;
    const rest = trimmed.slice(2).trim();
    if (rest.endsWith("$$") && rest.length > 2) {
      ranges.push({ from: lineStart, to: lineEnd, kind: "math-block", source: line });
    } else {
      blockMathStart = lineStart;
    }
  }

  // 复合编号行输出（跳过代码块范围）：编号 replace 为列表标记（保持源码编号），
  // 整行加缩进 padding（mark class 按嵌套深度，模拟 Obsidian 层级）。
  for (const c of compoundMarks) {
    if (codeRanges.some(([cf, ct]) => c.lineStart >= cf && c.lineEnd <= ct)) continue;
    ranges.push({
      from: c.from,
      to: c.to,
      kind: "listmark",
      ordinal: c.ordinal,
    });
    if (c.depth >= 1) {
      ranges.push({
        from: c.lineStart,
        to: c.lineEnd,
        kind: "compound-line",
        level: c.depth,
      });
    }
  }
  return ranges;
}

export function collectBlockRanges(view: EditorView): BlockRange[] {
  return collectBlockRangesFromState(view.state);
}

/** 块级范围 → Decoration。 */
function blockDecoration(r: BlockRange, todoBlocks: TodoBlock[] = []): Decoration {
  switch (r.kind) {
    case "code-block": {
      const source = r.source ?? "";
      const firstNewline = source.search(/\r?\n/);
      const lastNewline = source.lastIndexOf("\n");
      const code = firstNewline >= 0 && lastNewline > firstNewline
        ? source.slice(firstNewline + (source[firstNewline] === "\r" ? 2 : 1), lastNewline + 1)
        : "";
      return Decoration.replace({ block: true, widget: new CodeBlockWidget(code, r.language) });
    }
    case "math-block":
      return Decoration.replace({ block: true, widget: new MathBlockWidget(r.source ?? "", mathVersion.value) });
    case "todo-block":
      return Decoration.replace({ block: true, widget: new TodoBlockWidget(r.source ?? "", todoBlocks) });
    case "done-block":
      return Decoration.replace({ block: true, widget: new DoneBlockWidget(r.source ?? "", todoBlocks) });
    case "heading-mark":
      return Decoration.replace({ widget: new HeadingMarkWidget() });
    case "heading-line":
      return Decoration.mark({ class: `cm-live-h${r.level ?? 1}` });
    case "listmark":
      return Decoration.replace({
        widget: new ListMarkWidget(r.ordinal ?? "•"),
      });
    case "quote":
      return Decoration.replace({ widget: new QuoteMarkWidget() });
    case "quote-line":
      return Decoration.mark({ class: "cm-live-quote" });
    case "hr":
      return Decoration.replace({ widget: new HrWidget() });
    case "compound-line":
      return Decoration.mark({ class: `cm-live-n${r.level ?? 1}` });
  }
}

function selectionTouchesRange(state: EditorState, from: number, to: number): boolean {
  return state.selection.ranges.some((selection) => {
    if (selection.empty) return selection.from >= from && selection.from < to;
    return selection.from < to && selection.to > from;
  });
}

/**
 * 代码块会改变编辑器的垂直布局，必须由 StateField 直接提供，不能经 ViewPlugin。
 */
export function buildCodeBlockDecorations(state: EditorState): DecorationSet {
  const decorations = collectBlockRangesFromState(state)
    .filter((range) => range.kind === "code-block")
    .filter((range) => !selectionTouchesRange(state, range.from, range.to))
    .map((range) => blockDecoration(range).range(range.from, range.to));
  return Decoration.set(decorations, true);
}

export const codeBlockDecorationsField = StateField.define<DecorationSet>({
  create: buildCodeBlockDecorations,
  update(decorations, transaction) {
    if (transaction.docChanged || transaction.selection) {
      return buildCodeBlockDecorations(transaction.state);
    }
    return decorations;
  },
  provide: (field) => EditorView.decorations.from(field),
});

/** Todo 数据在 EditorState 内保存，避免不同便签窗口之间共享模块级状态。 */
export const setLiveTodoBlocks = StateEffect.define<TodoBlock[]>();

export const liveTodoBlocksField = StateField.define<TodoBlock[]>({
  create: () => [],
  update(todoBlocks, transaction) {
    for (const effect of transaction.effects) {
      if (effect.is(setLiveTodoBlocks)) return effect.value;
    }
    return todoBlocks;
  },
});

/** 需要占据独立行高的所有 Live Preview 块，由 StateField 提供 decoration。 */
export function buildLiveBlockDecorations(state: EditorState): DecorationSet {
  const todoBlocks = state.field(liveTodoBlocksField, false) ?? [];
  const decorations = collectBlockRangesFromState(state)
    .filter((range) => range.kind === "code-block" || range.kind === "math-block" || range.kind === "todo-block" || range.kind === "done-block")
    .filter((range) => !selectionTouchesRange(state, range.from, range.to))
    .map((range) => blockDecoration(range, todoBlocks).range(range.from, range.to));
  return Decoration.set(decorations, true);
}

export const liveBlockDecorationsField = StateField.define<DecorationSet>({
  create: buildLiveBlockDecorations,
  update(decorations, transaction) {
    const refresh = transaction.effects.some((effect) => effect.is(refreshLivePreview) || effect.is(setLiveTodoBlocks));
    if (transaction.docChanged || transaction.selection || refresh) {
      return buildLiveBlockDecorations(transaction.state);
    }
    return decorations;
  },
  provide: (field) => EditorView.decorations.from(field),
});

/** 构建 decoration 集：光标所在行不渲染（显示源码）。
 *  mark 类（标题整行样式）可与任何 decoration 重叠；replace 类做非重叠贪心。 */
export function buildLiveDecorations(view: EditorView): RangeSet<Decoration> {
  const doc = view.state.doc;
  const head = view.state.selection.main.head;
  const curLine = doc.lineAt(head);

  interface Item {
    from: number;
    to: number;
    deco: Decoration;
    isMark: boolean;
    isBlock: boolean;
  }
  const ranges: Item[] = [];
  const decorations: Range<Decoration>[] = [];
  // Obsidian 精确行为：行内元素只在「光标位于该元素内（含边界）或选区跨越」时
  // 显示源码标记，其余元素始终渲染（同行其他元素不受影响）；
  // 块级标记/任务 checkbox 按「光标所在行」显示源码。
  const sel = view.state.selection.main;
  for (const r of collectInlineRanges(view)) {
    if (r.kind === "task") {
      // 块级语义：光标所在行显示 [ ] 文本
      if (r.from < curLine.to && r.to > curLine.from) continue;
    } else if (r.className === "live-link") {
      // 链接整行规则：光标位于链接所在行时（无论光标在行内何处）显示源码
      // [text](url)，其余行渲染为可点击的链接文本。
      const linkLine = doc.lineAt(r.from);
      if (linkLine.number === curLine.number) continue;
    } else {
      const cursorIn = sel.from >= r.from && sel.from < r.to;
      const selOverlap = sel.from < r.to && sel.to > r.from;
      if (cursorIn || selOverlap) continue;
    }
    ranges.push({ from: r.from, to: r.to, deco: inlineDecoration(view, r), isMark: false, isBlock: false });
  }
  for (const r of collectBlockRanges(view)) {
    if (r.kind === "code-block" || r.kind === "math-block" || r.kind === "todo-block" || r.kind === "done-block") {
      // 影响垂直布局的 block 均由 liveBlockDecorationsField 直接提供。
      continue;
    }
    ranges.push({
      from: r.from,
      to: r.to,
      deco: blockDecoration(r),
      isMark: r.kind === "heading-line" || r.kind === "quote-line" || r.kind === "compound-line",
      isBlock: true,
    });
  }
  ranges.sort((a, b) => a.from - b.from || b.to - a.to);

  const replaceAccepted: Item[] = [];
  for (const r of ranges) {
    // 块级标记：光标所在行显示源码（行内元素已按 Obsidian 元素级规则判断过）
    if (r.isBlock && r.from < curLine.to && r.to > curLine.from) continue;
    if (r.isMark) {
      decorations.push(r.deco.range(r.from, r.to));
      continue;
    }
    // replace 类：非重叠贪心（外层优先，嵌套内层丢弃）
    const prev = replaceAccepted[replaceAccepted.length - 1];
    if (prev && r.from < prev.to) continue;
    replaceAccepted.push(r);
    decorations.push(r.deco.range(r.from, r.to));
  }
  return RangeSet.of(decorations, true);
}

/** 编辑器扩展：doc/selection 变化时重算 decoration（ViewPlugin 持有真实 view，
 *  checkbox widget 点击时可用 view.dispatch 修改文档）。 */
export const liveDecorationsPlugin = ViewPlugin.fromClass(
  class {
    decorations: RangeSet<Decoration>;

    constructor(view: EditorView) {
      this.decorations = buildLiveDecorations(view);
    }

    update(update: import("@codemirror/view").ViewUpdate) {
      const rendererReady = update.transactions.some((transaction) =>
        transaction.effects.some((effect) => effect.is(refreshLivePreview)),
      );
      if (update.docChanged || update.selectionSet || update.viewportChanged || rendererReady) {
        this.decorations = buildLiveDecorations(update.view);
      }
    }
  },
  {
    decorations: (v) => v.decorations,
  },
);
