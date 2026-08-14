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

/** 构建 decoration 集：光标所在行不渲染（显示源码）。 */
export function buildLiveDecorations(view: EditorView): RangeSet<Decoration> {
  const builder = new RangeSetBuilder<Decoration>();
  const doc = view.state.doc;
  const head = view.state.selection.main.head;
  const curLine = doc.lineAt(head);
  for (const r of collectInlineRanges(view)) {
    // 与光标行相交 → 跳过（显示源码）
    if (r.from < curLine.to && r.to > curLine.from) continue;
    builder.add(r.from, r.to, inlineDecoration(view, r));
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
