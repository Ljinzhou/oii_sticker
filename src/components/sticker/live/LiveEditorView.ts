// 及时预览编辑器的 CodeMirror 6 内核（Obsidian Live Preview 同款技术栈）。
// Phase A：基础集成——行号/折行/缩进/markdown 语法高亮、双向同步、Ctrl+S、字号自适应。
// 注：每个便签窗口是独立 webview，模块级单例 compartment 在同一窗口内唯一。
import { Compartment, EditorState, Prec } from "@codemirror/state";
import {
  EditorView,
  crosshairCursor,
  drawSelection,
  dropCursor,
  highlightActiveLine,
  highlightSpecialChars,
  keymap,
  rectangularSelection,
} from "@codemirror/view";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import {
  HighlightStyle,
  bracketMatching,
  defaultHighlightStyle,
  indentOnInput,
  syntaxHighlighting,
} from "@codemirror/language";
import {
  defaultKeymap,
  history,
  historyKeymap,
  indentWithTab,
} from "@codemirror/commands";
import {
  autocompletion,
  closeBrackets,
  closeBracketsKeymap,
  completionKeymap,
} from "@codemirror/autocomplete";
import { tags } from "@lezer/highlight";
import { fontFamilyTheme, fontSizeTheme, lightTheme, makeShowLineNumbers } from "./editorTheme";
import { reportSlash } from "./editorSlash";
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
  /** 是否显示行号（系统设置 editor_line_numbers 统一控制，默认 true）。 */
  showLineNumbers?: boolean;
  /** 文档变化（用户编辑）→ 防抖回写由调用方处理。 */
  onDocChange: (doc: string) => void;
  /** Ctrl+S。 */
  onSave: () => void;
  /** 及时预览内检测到 / 查询。 */
  onSlash?: (query: string, from: number, to: number, anchor: SlashAnchor) => void;
  onSlashClose?: () => void;
  onTodoOpen?: (id: string) => void;
  todoBlocks?: TodoBlock[];
  /** 斜杠菜单是否打开：打开时 ↑/↓/Enter/Esc 交给菜单处理，不再移动光标/换行。 */
  slashOpen?: () => boolean;
  onSlashNav?: (dir: 1 | -1) => void;
  onSlashConfirm?: () => void;
  onSlashCancel?: () => void;
}

const liveHighlightStyle = HighlightStyle.define([
  { tag: tags.heading, textDecoration: "none" },
]);

/**
 * 与 basicSetup 等价的基础扩展，区别：不内置 lineNumbers()/foldGutter()——
 * 行号由 makeShowLineNumbers 的 compartment 统一控制（开关来自系统设置
 * editor_line_numbers，与 Markdown 编辑模式共用同一份行号显示代码）。
 */
const liveBaseSetup = [
  highlightSpecialChars(),
  history(),
  drawSelection(),
  dropCursor(),
  EditorState.allowMultipleSelections.of(true),
  indentOnInput(),
  syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
  bracketMatching(),
  closeBrackets(),
  autocompletion(),
  rectangularSelection(),
  crosshairCursor(),
  highlightActiveLine(),
  keymap.of([
    ...closeBracketsKeymap,
    ...defaultKeymap,
    ...historyKeymap,
    ...completionKeymap,
  ]),
];

/** 字号主题（compartment 热替换）。 */
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
let lineNumberCompartment: Compartment | null = null;

/** 创建 CM6 编辑器实例。 */
export function createLiveView(parent: HTMLElement, opts: LiveViewOptions): EditorView {
  fontSizeCompartment = new Compartment();
  fontFamilyCompartment = new Compartment();
  lineNumberCompartment = new Compartment();
  const state = EditorState.create({
    doc: opts.doc,
    extensions: [
      liveBaseSetup,
      markdown({ base: markdownLanguage }), // 含 GFM：删除线/任务列表/表格/自动链接
      syntaxHighlighting(liveHighlightStyle),
      lineNumberCompartment.of(makeShowLineNumbers(opts.showLineNumbers ?? true)),
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
            return runLiveTransform(view, buildEnterTransaction);
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
        { key: "Tab", run: (view) => runTableOrLiveTransform(view, 1) },
        { key: "Shift-Tab", run: (view) => runTableOrLiveTransform(view, -1) },
        { key: "Backspace", run: (view) => runLiveTransform(view, buildBackspaceTransaction) },
        { key: "Mod-b", run: (view) => runFormat(view, "**", "**", "input.format.bold") },
        { key: "Mod-i", run: (view) => runFormat(view, "*", "*", "input.format.italic") },
        { key: "Mod-Shift-x", run: (view) => runFormat(view, "~~", "~~", "input.format.strike") },
        indentWithTab,
        { key: "Mod-s", run: () => { opts.onSave(); return true; } },
      ])),
      EditorView.lineWrapping,
      liveTodoBlocksField.init(() => opts.todoBlocks ?? []),
      liveBlockDecorationsField,
      liveDecorationsPlugin,
      EditorView.updateListener.of((u) => {
        if (u.docChanged) {
          const doc = u.state.doc.toString();
          opts.onDocChange(doc);
        }
        if (u.docChanged || u.selectionSet) reportSlash(u.view, (query, from, to, anchor) => opts.onSlash?.(query, from, to, anchor), () => opts.onSlashClose?.());
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

/** 更新行号显示（与 Markdown 编辑模式共用同一个 editor_line_numbers 开关）。 */
export function setLiveLineNumbers(view: EditorView, show: boolean): void {
  if (!lineNumberCompartment) return;
  view.dispatch({
    effects: lineNumberCompartment.reconfigure(makeShowLineNumbers(show)),
  });
}

/** 外部 Todo 更新后刷新实时预览中的受控任务卡片。 */
export function setLiveTodoBlocksInView(view: EditorView, todoBlocks: TodoBlock[]): void {
  view.dispatch({ effects: setLiveTodoBlocks.of(todoBlocks) });
}
