// Markdown 编辑模式的 CodeMirror 源码内核（原生文本编辑，不渲染 inline decoration）。
// 与及时预览共用同一套代码：
//  - 行号显示：makeShowLineNumbers（复用「及时预览的行号显示代码」）
//  - 主题：lightTheme / fontSizeTheme / fontFamilyTheme
//  - 斜杠查询：reportSlash
//  - 智能编辑（Enter/Tab/Shift+Tab）：liveTransforms（同一份 edit-actions 包装）
import { Compartment, EditorState, Prec } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { tags } from "@lezer/highlight";
import { fontFamilyTheme, fontSizeTheme, lightTheme, makeShowLineNumbers } from "./editorTheme";
import { buildEnterTransaction, buildShiftTabTransaction, buildTabTransaction } from "./liveTransforms";
import { reportSlash } from "./editorSlash";
import { applyExternalDoc } from "./externalDoc";
import type { SlashAnchor } from "../../slash/types";

export interface MarkdownSourceOptions {
  doc: string;
  fontSize: number;
  fontFamily?: string;
  showLineNumbers: boolean;
  onDocChange: (doc: string) => void;
  onSlash?: (query: string, from: number, to: number, anchor: SlashAnchor) => void;
  onSlashClose?: () => void;
  onTodoOpen?: (id: string) => void;
  /** 斜杠菜单是否打开：打开时 ↑/↓/Enter/Esc 交给菜单处理，不再移动光标/换行。 */
  slashOpen?: () => boolean;
  onSlashNav?: (dir: 1 | -1) => void;
  onSlashConfirm?: () => void;
  onSlashCancel?: () => void;
}

/** Markdown 源码语法着色（与旧版 markdownSyntaxHighlight 配色保持一致）。 */
const sourceHighlightStyle = HighlightStyle.define([
  { tag: tags.heading, color: "#a855f7", fontWeight: "600" },
  { tag: tags.processingInstruction, color: "#4f7cff", fontWeight: "600" },
  { tag: tags.quote, color: "#9ca3af" },
  { tag: tags.contentSeparator, color: "#9ca3af" },
  { tag: tags.monospace, color: "#d97706" },
  { tag: tags.strong, color: "#b45309", fontWeight: "700" },
  { tag: tags.emphasis, color: "#b45309", fontStyle: "italic" },
  { tag: tags.link, color: "#2563eb" },
  { tag: tags.url, color: "#7c9cf0" },
  { tag: tags.strikethrough, color: "#9ca3af", textDecoration: "line-through" },
]);

let fontSizeCompartment: Compartment | null = null;
let fontFamilyCompartment: Compartment | null = null;
let lineNumbersCompartment: Compartment | null = null;

/** 多行选区整体缩进（每行前加 2 空格，逻辑与旧 textarea 版一致）；单行 Tab 走 buildTabTransaction。 */
function indentSelection(view: EditorView): boolean {
  const { from, to } = view.state.selection.main;
  if (from === to) return false;
  const doc = view.state.doc.toString();
  const prefix = doc.slice(0, from);
  const selected = doc.slice(from, to);
  const indented = selected
    .split("\n")
    .map((line, i) => (i === 0 && prefix.endsWith("\n") ? "  " + line : i > 0 ? "  " + line : line))
    .join("\n");
  const added = indented.length - selected.length;
  view.dispatch({
    changes: { from, to, insert: indented },
    selection: { anchor: from + (prefix.endsWith("\n") ? 2 : 0), head: to + added },
    userEvent: "input.indent",
  });
  return true;
}

/** 点击位置 → 所在 <todo-block> 的 id（逻辑与旧 textarea 版一致）。 */
function todoIdAt(view: EditorView, pos: number): string | undefined {
  const doc = view.state.doc.toString();
  if (pos < 0 || pos > doc.length) return undefined;
  const start = doc.lastIndexOf("<todo-block", pos);
  if (start < 0) return undefined;
  const end = doc.indexOf("</todo-block>", start);
  if (end < 0 || end < pos) return undefined;
  const tag = doc.slice(start, end + "</todo-block>".length);
  return /\bid=["']([^"']+)["']/.exec(tag)?.[1];
}

/** 创建 Markdown 源码编辑器（CodeMirror 6 内核）。 */
export function createMarkdownSourceView(parent: HTMLElement, opts: MarkdownSourceOptions): EditorView {
  fontSizeCompartment = new Compartment();
  fontFamilyCompartment = new Compartment();
  lineNumbersCompartment = new Compartment();

  const state = EditorState.create({
    doc: opts.doc,
    extensions: [
      lineNumbersCompartment.of(makeShowLineNumbers(opts.showLineNumbers)),
      markdown({ base: markdownLanguage }), // GFM 语法高亮
      syntaxHighlighting(sourceHighlightStyle),
      history(),
      // 自定义键位设为最高优先级：markdown 语言自带 Prec.high 的 Enter（列表续行）必须在斜杠菜单之后生效
      Prec.highest(keymap.of([
        // 斜杠菜单打开时优先接管键盘：上下切换菜单项、回车选中、Esc 关闭
        {
          key: "ArrowDown",
          run: () => {
            if (!opts.slashOpen?.()) return false;
            opts.onSlashNav?.(1);
            return true;
          },
        },
        {
          key: "ArrowUp",
          run: () => {
            if (!opts.slashOpen?.()) return false;
            opts.onSlashNav?.(-1);
            return true;
          },
        },
        {
          key: "Enter",
          run: (view) => {
            // 斜杠菜单打开：回车 = 选中当前高亮项；关闭：走列表续行等智能变换
            if (opts.slashOpen?.()) {
              opts.onSlashConfirm?.();
              return true;
            }
            const spec = buildEnterTransaction(view.state.doc.toString(), view.state.selection.main);
            if (!spec) return false;
            view.dispatch(spec);
            return true;
          },
        },
        {
          key: "Escape",
          run: () => {
            if (!opts.slashOpen?.()) return false;
            opts.onSlashCancel?.();
            return true;
          },
        },
        {
          key: "Tab",
          run: (view) => {
            if (!view.state.selection.main.empty) return indentSelection(view);
            const spec = buildTabTransaction(view.state.doc.toString(), view.state.selection.main);
            if (!spec) return false;
            view.dispatch(spec);
            return true;
          },
        },
        {
          key: "Shift-Tab",
          run: (view) => {
            const spec = buildShiftTabTransaction(view.state.doc.toString(), view.state.selection.main);
            if (!spec) return false;
            view.dispatch(spec);
            return true;
          },
        },
      ])),
      keymap.of([...historyKeymap, ...defaultKeymap]),
      EditorView.lineWrapping,
      EditorView.contentAttributes.of({ spellcheck: "false" }),
      EditorView.updateListener.of((u) => {
        if (u.docChanged) opts.onDocChange(u.state.doc.toString());
        if (u.docChanged || u.selectionSet) {
          reportSlash(
            u.view,
            (query, from, to, anchor) => opts.onSlash?.(query, from, to, anchor),
            () => opts.onSlashClose?.(),
          );
        }
      }),
      fontSizeCompartment.of(fontSizeTheme(opts.fontSize)),
      fontFamilyCompartment.of(fontFamilyTheme(opts.fontFamily ?? "Microsoft YaHei")),
      lightTheme,
    ],
  });

  const view = new EditorView({ state, parent });
  view.dom.addEventListener("click", (event) => {
    const target = event.target as HTMLElement | null;
    if (!target?.closest(".cm-content")) return;
    const pos = view.posAtCoords({ x: event.clientX, y: event.clientY });
    const id = pos == null ? undefined : todoIdAt(view, pos);
    if (id) opts.onTodoOpen?.(id);
  });
  return view;
}

/** 外部内容更新 → 同步进编辑器（最小差异替换，保留光标；组词期间跳过）。 */
export function setMarkdownSourceDoc(view: EditorView, doc: string): void {
  applyExternalDoc(view, doc);
}

/** 更新编辑字号（theme compartment 热替换）。 */
export function setMarkdownSourceFontSize(view: EditorView, fontSize: number): void {
  if (!fontSizeCompartment) return;
  view.dispatch({ effects: fontSizeCompartment.reconfigure(fontSizeTheme(fontSize)) });
}

/** 更新编辑字体（theme compartment 热替换）。 */
export function setMarkdownSourceFontFamily(view: EditorView, fontFamily: string): void {
  if (!fontFamilyCompartment) return;
  view.dispatch({ effects: fontFamilyCompartment.reconfigure(fontFamilyTheme(fontFamily)) });
}

/** 更新行号显示（与及时预览共用 makeShowLineNumbers）。 */
export function setMarkdownSourceLineNumbers(view: EditorView, show: boolean): void {
  if (!lineNumbersCompartment) return;
  view.dispatch({ effects: lineNumbersCompartment.reconfigure(makeShowLineNumbers(show)) });
}
