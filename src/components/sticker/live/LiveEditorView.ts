// 及时预览编辑器的 CodeMirror 6 内核（Obsidian Live Preview 同款技术栈）。
// Phase A：基础集成——行号/折行/缩进/markdown 语法高亮、双向同步、Ctrl+S、字号自适应。
// 注：每个便签窗口是独立 webview，模块级单例 compartment 在同一窗口内唯一。
import { Compartment, EditorState } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";
import { basicSetup } from "codemirror";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { indentWithTab } from "@codemirror/commands";
import { tags } from "@lezer/highlight";
import { liveBlockDecorationsField, liveDecorationsPlugin, liveTodoBlocksField, setLiveTodoBlocks } from "./liveDecorations";
import { refreshLivePreview } from "./liveEffects";
import {
  buildBackspaceTransaction,
  buildEnterTransaction,
  buildShiftTabTransaction,
  buildTabTransaction,
  buildTableBackwardTransaction,
  buildTableForwardTransaction,
  buildWrapTransaction,
} from "./liveTransforms";
import { mathInstancePromise } from "../../../utils/markdown";
import type { TodoBlock } from "../../../types";
import type { SlashAnchor } from "../../slash/types";

export interface LiveViewOptions {
  doc: string;
  fontSize: number;
  fontFamily?: string;
  /** 文档变化（用户编辑）→ 防抖回写由调用方处理。 */
  onDocChange: (doc: string) => void;
  /** Ctrl+S。 */
  onSave: () => void;
  /** 及时预览内检测到 / 查询。 */
  onSlash?: (query: string, from: number, to: number, anchor: SlashAnchor) => void;
  onSlashClose?: () => void;
  onTodoOpen?: (id: string) => void;
  todoBlocks?: TodoBlock[];
}

/** 便签浅色背景下的编辑器主题（透明背景、继承颜色、细行号）。 */
const lightTheme = EditorView.theme({
  "&": {
    backgroundColor: "transparent",
    color: "inherit",
    height: "100%",
  },
  "&.cm-focused": { outline: "none" },
  ".cm-scroller": {
    lineHeight: "1.7",
  },
  ".cm-content": {
    caretColor: "#333",
    padding: "8px 6px",
  },
  ".cm-gutters": {
    backgroundColor: "transparent",
    border: "none",
    color: "rgba(0, 0, 0, 0.3)",
  },
  ".cm-line": { padding: "0" },
  ".cm-selectionBackground, &.cm-focused .cm-selectionBackground": {
    backgroundColor: "rgba(79, 124, 255, 0.25) !important",
  },
});

const liveHighlightStyle = HighlightStyle.define([
  { tag: tags.heading, textDecoration: "none" },
]);

/** 字号主题（compartment 热替换）。 */
function fontSizeTheme(size: number) {
  return EditorView.theme({ "&": { fontSize: `${size}px` } });
}

function runLiveTransform(
  view: EditorView,
  build: typeof buildEnterTransaction,
): boolean {
  const spec = build(view.state.doc.toString(), view.state.selection.main);
  if (!spec) return false;
  view.dispatch(spec);
  return true;
}

function runTableOrLiveTransform(view: EditorView, direction: -1 | 1): boolean {
  const build = direction > 0 ? buildTableForwardTransaction : buildTableBackwardTransaction;
  if (runLiveTransform(view, build)) return true;
  return runLiveTransform(view, direction > 0 ? buildTabTransaction : buildShiftTabTransaction);
}

function runFormat(view: EditorView, prefix: string, suffix: string, userEvent: string): boolean {
  view.dispatch(buildWrapTransaction(view.state.doc.toString(), view.state.selection.main, prefix, suffix, userEvent));
  return true;
}

let fontSizeCompartment: Compartment | null = null;
let fontFamilyCompartment: Compartment | null = null;

function fontFamilyTheme(family: string) {
  return EditorView.theme({
    ".cm-scroller, .cm-content, .cm-gutters": { fontFamily: family },
  });
}

function slashAnchorAtSelection(view: EditorView): SlashAnchor {
  const coords = view.coordsAtPos(view.state.selection.main.head);
  const host = view.dom.getBoundingClientRect();
  if (!coords) return { left: 6, top: 30 };
  return {
    left: Math.max(0, coords.left - host.left),
    top: Math.max(0, coords.bottom - host.top + 6),
  };
}

function reportSlash(view: EditorView, opts: LiveViewOptions): void {
  const doc = view.state.doc.toString();
  const head = view.state.selection.main.head;
  const match = /(?:^|\n)\/([^\s\/]*)$/.exec(doc.slice(0, head));
  if (!match) {
    opts.onSlashClose?.();
    return;
  }
  const from = head - match[0].length + (match[0].startsWith("\n") ? 1 : 0);
  opts.onSlash?.(match[1], from, head, slashAnchorAtSelection(view));
}

/** 创建 CM6 编辑器实例。 */
export function createLiveView(parent: HTMLElement, opts: LiveViewOptions): EditorView {
  fontSizeCompartment = new Compartment();
  fontFamilyCompartment = new Compartment();
  const state = EditorState.create({
    doc: opts.doc,
    extensions: [
      basicSetup,
      markdown({ base: markdownLanguage }), // 含 GFM：删除线/任务列表/表格/自动链接
      syntaxHighlighting(liveHighlightStyle),
      keymap.of([
        { key: "Enter", run: (view) => runLiveTransform(view, buildEnterTransaction) },
        { key: "Tab", run: (view) => runTableOrLiveTransform(view, 1) },
        { key: "Shift-Tab", run: (view) => runTableOrLiveTransform(view, -1) },
        { key: "Backspace", run: (view) => runLiveTransform(view, buildBackspaceTransaction) },
        { key: "Mod-b", run: (view) => runFormat(view, "**", "**", "input.format.bold") },
        { key: "Mod-i", run: (view) => runFormat(view, "*", "*", "input.format.italic") },
        { key: "Mod-Shift-x", run: (view) => runFormat(view, "~~", "~~", "input.format.strike") },
        indentWithTab,
        { key: "Mod-s", run: () => { opts.onSave(); return true; } },
      ]),
      EditorView.lineWrapping,
      liveTodoBlocksField.init(() => opts.todoBlocks ?? []),
      liveBlockDecorationsField,
      liveDecorationsPlugin,
      EditorView.updateListener.of((u) => {
        if (u.docChanged) {
          const doc = u.state.doc.toString();
          opts.onDocChange(doc);
        }
        if (u.docChanged || u.selectionSet) reportSlash(u.view, opts);
      }),
      fontSizeCompartment.of(fontSizeTheme(opts.fontSize)),
      fontFamilyCompartment.of(fontFamilyTheme(opts.fontFamily ?? "Microsoft YaHei")),
      lightTheme,
    ],
  });
  const view = new EditorView({ state, parent });
  view.dom.addEventListener("click", (event) => {
    const target = event.target as HTMLElement | null;
    const card = target?.closest<HTMLElement>(".todo-block-card");
    const id = card?.dataset.todoId;
    if (id) opts.onTodoOpen?.(id);
  });
  void mathInstancePromise.then(() => {
    if (view.dom.isConnected) {
      try {
        view.dispatch({ effects: refreshLivePreview.of(null) });
      } catch {
        // MathJax may resolve during component teardown; ignore a late refresh.
      }
    }
  });
  return view;
}

/** 外部内容更新 → 同步进编辑器（内容相同则跳过，避免自身回写触发循环）。 */
export function setLiveDoc(view: EditorView, doc: string): void {
  if (view.state.doc.toString() === doc) return;
  view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: doc } });
}

/** 更新编辑字号（theme compartment 热替换）。 */
export function setLiveFontSize(view: EditorView, fontSize: number): void {
  if (!fontSizeCompartment) return;
  view.dispatch({
    effects: fontSizeCompartment.reconfigure(fontSizeTheme(fontSize)),
  });
}

/** 更新编辑字体（theme compartment 热替换）。 */
export function setLiveFontFamily(view: EditorView, fontFamily: string): void {
  if (!fontFamilyCompartment) return;
  view.dispatch({
    effects: fontFamilyCompartment.reconfigure(fontFamilyTheme(fontFamily)),
  });
}

/** 外部 Todo 更新后刷新实时预览中的受控任务卡片。 */
export function setLiveTodoBlocksInView(view: EditorView, todoBlocks: TodoBlock[]): void {
  view.dispatch({ effects: setLiveTodoBlocks.of(todoBlocks) });
}
