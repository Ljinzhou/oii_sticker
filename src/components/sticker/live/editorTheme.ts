// 编辑器共享主题与行号扩展（及时预览 / Markdown 两种编辑模式共用同一份代码）。
import type { Extension } from "@codemirror/state";
import { EditorView, lineNumbers } from "@codemirror/view";
import { foldGutter } from "@codemirror/language";

/** 行号栏扩展：true 返回 CodeMirror 标准行号 + 折叠箭头（标题行前的小三角，点击可折叠），
 *  false 整个行号栏隐藏。与及时预览 basicSetup 同源，Markdown 编辑模式直接复用本函数。 */
export function makeShowLineNumbers(show: boolean): Extension {
  return show ? [lineNumbers(), foldGutter()] : [];
}

/** 便签浅色背景下的编辑器主题（透明背景、继承颜色、细行号）。 */
export const lightTheme = EditorView.theme({
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

/** 字号主题（compartment 热替换）。 */
export function fontSizeTheme(size: number) {
  return EditorView.theme({ "&": { fontSize: `${size}px` } });
}

/** 字体主题（compartment 热替换）。 */
export function fontFamilyTheme(family: string) {
  return EditorView.theme({
    ".cm-scroller, .cm-content, .cm-gutters": { fontFamily: family },
  });
}
