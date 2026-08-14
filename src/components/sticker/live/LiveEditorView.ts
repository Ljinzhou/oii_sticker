// 即时预览编辑器的 CodeMirror 6 内核（Obsidian Live Preview 同款技术栈）。
// Phase A：基础集成——行号/折行/缩进/markdown 语法高亮、双向同步、Ctrl+S、字号自适应。
// 注：每个便签窗口是独立 webview，模块级单例 compartment 在同一窗口内唯一。
import { Compartment, EditorState } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";
import { basicSetup } from "codemirror";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { indentWithTab } from "@codemirror/commands";
import { liveDecorationsPlugin } from "./liveDecorations";

export interface LiveViewOptions {
  doc: string;
  fontSize: number;
  /** 文档变化（用户编辑）→ 防抖回写由调用方处理。 */
  onDocChange: (doc: string) => void;
  /** Ctrl+S。 */
  onSave: () => void;
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
    fontFamily: "Consolas, 'Courier New', monospace",
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
    fontFamily: "Consolas, 'Courier New', monospace",
  },
  ".cm-line": { padding: "0" },
  ".cm-selectionBackground, &.cm-focused .cm-selectionBackground": {
    backgroundColor: "rgba(79, 124, 255, 0.25) !important",
  },
});

/** 字号主题（compartment 热替换）。 */
function fontSizeTheme(size: number) {
  return EditorView.theme({ "&": { fontSize: `${size}px` } });
}

let fontSizeCompartment: Compartment | null = null;

/** 创建 CM6 编辑器实例。 */
export function createLiveView(parent: HTMLElement, opts: LiveViewOptions): EditorView {
  fontSizeCompartment = new Compartment();
  const state = EditorState.create({
    doc: opts.doc,
    extensions: [
      basicSetup,
      markdown({ base: markdownLanguage }), // 含 GFM：删除线/任务列表/表格/自动链接
      keymap.of([
        indentWithTab,
        { key: "Mod-s", run: () => { opts.onSave(); return true; } },
      ]),
      EditorView.lineWrapping,
      liveDecorationsPlugin,
      EditorView.updateListener.of((u) => {
        if (u.docChanged) opts.onDocChange(u.state.doc.toString());
      }),
      fontSizeCompartment.of(fontSizeTheme(opts.fontSize)),
      lightTheme,
    ],
  });
  return new EditorView({ state, parent });
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
