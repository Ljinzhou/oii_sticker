// 即时预览的行内渲染 decoration（Live Preview 核心）。
// 机制（Obsidian Live Preview 同思路）：
// - 用 lezer 语法树（@codemirror/lang-markdown 自带）提取行内元素
//   （StrongEmphasis/Emphasis/Strikethrough/InlineCode/Link），
//   数学公式（$..$）用正则补充扫描（lezer 默认不识别 $）；
// - 光标所在行的元素**不渲染**（显示源码，便于编辑标记），其余行渲染；
// - 嵌套元素只渲染最外层（非重叠贪心）；
// - 渲染用 markdown-it 片段渲染（保证嵌套/公式正确），输出为 replace widget。
import { RangeSet, RangeSetBuilder } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin, WidgetType } from "@codemirror/view";
import { ensureSyntaxTree, syntaxTree } from "@codemirror/language";
import { renderMarkdownEditable } from "../../../utils/markdown-editable";

/** 渲染 widget：把行内元素片段替换为渲染 DOM。 */
class InlineRenderWidget extends WidgetType {
  constructor(
    readonly html: string,
    readonly cls: string,
  ) {
    super();
  }

  eq(other: InlineRenderWidget) {
    return other.html === this.html && other.cls === this.cls;
  }

  toDOM() {
    const span = document.createElement("span");
    span.className = `live-render ${this.cls}`;
    span.innerHTML = this.html;
    return span;
  }

  ignoreEvent() {
    return false;
  }
}

/** 任务 checkbox widget：点击切换 [ ] ↔ [x]（直接改文档，保存链路自动落库）。 */
class TaskCheckboxWidget extends WidgetType {
  constructor(
    readonly checked: boolean,
    readonly view: EditorView,
  ) {
    super();
  }

  eq(other: TaskCheckboxWidget) {
    return other.checked === this.checked;
  }

  toDOM() {
    const cb = document.createElement("input");
    cb.type = "checkbox";
    cb.className = "live-task-checkbox";
    cb.checked = this.checked;
    cb.addEventListener("click", (e) => {
      e.preventDefault();
      const from = this.view.state.selection.main.from;
      const line = this.view.state.doc.lineAt(from);
      // 定位当前行任务标记并翻转
      const m = /^(\s*[-*+]\s+)\[([ xX])\]/.exec(line.text);
      if (!m) return;
      const marker = m[2] === " " ? "[x]" : "[ ]";
      this.view.dispatch({
        changes: {
          from: line.from + m[0].length - 3,
          to: line.from + m[0].length,
          insert: marker,
        },
      });
      this.view.focus();
    });
    return cb;
  }

  ignoreEvent() {
    return false;
  }
}

/** 列表标记 widget：无序显示圆点，有序显示计算编号（支持 1. / 1.1. 嵌套）。 */
class ListMarkWidget extends WidgetType {
  constructor(readonly text: string) {
    super();
  }

  eq(other: ListMarkWidget) {
    return other.text === this.text;
  }

  toDOM() {
    const span = document.createElement("span");
    span.className = "live-listmark";
    span.textContent = this.text;
    return span;
  }

  ignoreEvent() {
    return false;
  }
}

/** 分隔线 widget。 */
class HrWidget extends WidgetType {
  eq() {
    return true;
  }

  toDOM() {
    const div = document.createElement("div");
    div.className = "live-hr";
    return div;
  }

  ignoreEvent() {
    return false;
  }
}

/** 从 markdown-it 片段渲染提取 <p> 内 HTML。 */
function renderFragment(src: string): string {
  const html = renderMarkdownEditable(src);
  const m = /<p>(.*?)<\/p>\s*$/s.exec(html);
  return m ? m[1] : html;
}

interface InlineRange {
  from: number;
  to: number;
  kind: "render" | "task" | "math";
  checked?: boolean;
}

/** 块级渲染范围：标题标记/列表标记/引用标记/分隔线。 */
interface BlockRange {
  from: number;
  to: number;
  kind: "heading-mark" | "heading-line" | "listmark" | "quote" | "quote-line" | "hr" | "compound-line";
  level?: number; // heading 级别 / 复合编号嵌套深度
  ordinal?: string; // 有序列表显示编号（如 "1."、"1.1."）；无序为 "•"
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
            : { from, to, kind },
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
      widget: new TaskCheckboxWidget(r.checked ?? false, view),
    });
  }
  if (r.kind === "math") {
    return Decoration.replace({
      widget: new InlineRenderWidget(renderFragment(text), "live-math"),
    });
  }
  const clsMap: Record<string, string> = {
    StrongEmphasis: "live-strong",
    Emphasis: "live-em",
    Strikethrough: "live-del",
    Link: "live-link",
  };
  const cls = clsMap[r.kind] ?? "";
  return Decoration.replace({
    widget: new InlineRenderWidget(renderFragment(text), cls),
  });
}

/** 收集块级渲染范围（标题/列表标记/引用/分隔线）。
 *  有序列表编号按嵌套层级计算（1. / 1.1. / 1.1.1.）。 */
export function collectBlockRanges(view: EditorView): BlockRange[] {
  const state = view.state;
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
        ranges.push({
          from,
          to,
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

/** 标题标记 widget（隐藏 # 符号，占位保持行内布局）。 */
class HeadingMarkWidget extends WidgetType {
  eq() {
    return true;
  }
  toDOM() {
    const s = document.createElement("span");
    s.className = "live-heading-mark";
    return s;
  }
  ignoreEvent() {
    return true;
  }
}

/** 引用标记 widget（隐藏 > 符号）。 */
class QuoteMarkWidget extends WidgetType {
  eq() {
    return true;
  }
  toDOM() {
    const s = document.createElement("span");
    s.className = "live-quote-mark";
    s.textContent = " ";
    return s;
  }
  ignoreEvent() {
    return true;
  }
}

/** 块级范围 → Decoration。 */
function blockDecoration(r: BlockRange): Decoration {
  switch (r.kind) {
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

/** 构建 decoration 集：光标所在行不渲染（显示源码）。
 *  mark 类（标题整行样式）可与任何 decoration 重叠；replace 类做非重叠贪心。 */
export function buildLiveDecorations(view: EditorView): RangeSet<Decoration> {
  const builder = new RangeSetBuilder<Decoration>();
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
  // Obsidian 精确行为：行内元素只在「光标位于该元素内（含边界）或选区跨越」时
  // 显示源码标记，其余元素始终渲染（同行其他元素不受影响）；
  // 块级标记/任务 checkbox 按「光标所在行」显示源码。
  const sel = view.state.selection.main;
  for (const r of collectInlineRanges(view)) {
    if (r.kind === "task") {
      // 块级语义：光标所在行显示 [ ] 文本
      if (r.from < curLine.to && r.to > curLine.from) continue;
    } else {
      const cursorIn = sel.from >= r.from && sel.from <= r.to;
      const selOverlap = sel.from < r.to && sel.to > r.from;
      if (cursorIn || selOverlap) continue;
    }
    ranges.push({ from: r.from, to: r.to, deco: inlineDecoration(view, r), isMark: false, isBlock: false });
  }
  for (const r of collectBlockRanges(view)) {
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
      builder.add(r.from, r.to, r.deco);
      continue;
    }
    // replace 类：非重叠贪心（外层优先，嵌套内层丢弃）
    const prev = replaceAccepted[replaceAccepted.length - 1];
    if (prev && r.from < prev.to) continue;
    replaceAccepted.push(r);
    builder.add(r.from, r.to, r.deco);
  }
  return builder.finish();
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
      if (update.docChanged || update.selectionSet || update.viewportChanged) {
        this.decorations = buildLiveDecorations(update.view);
      }
    }
  },
  {
    decorations: (v) => v.decorations,
  },
);
