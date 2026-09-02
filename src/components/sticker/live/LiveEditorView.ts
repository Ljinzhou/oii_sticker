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
  type ViewUpdate,
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
import { applyExternalDoc } from "./externalDoc";
import { liveBlockDecorationsField, liveDecorationsPlugin, liveTodoBlocksField, setLiveTodoBlocks } from "./liveDecorations";
import { refreshLivePreview } from "./liveEffects";
import {
  liveAtomicRangesField,
  liveUiStateField,
  setLiveUiState,
} from "./liveDecorations";
import { DEFAULT_BLOCK_UI, type BlockUiState } from "../../../utils/block-ui";
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
import { invoke } from "../../../composables/useTauri";
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
  /** 用户折叠状态快照（卡片折叠/子任务显隐/已完成来源），经 effect 热替换。 */
  ui?: BlockUiState;
  /** UI 状态的便签键（stickerId 字符串）。 */
  uiKey?: string;
  /**
   * 卡片内用户交互（点击下拉箭头等）：foldCard=任务块整体折叠、
   * foldSub=某任务的子任务显隐、doneFold=已完成列表折叠。
   * 持久化由调用方完成，编辑器只上报语义事件。
   */
  onBlockUiAction?: (action: "foldCard" | "foldSub" | "doneFold", id: string) => void;
  /** 已完成任务卡来源切换（select.change；"" = 全部任务）。 */
  onDoneSource?: (sourceId: string, uiKey: string) => void;
  /**
   * 粘贴链接时获取网页标题（默认走 Tauri `fetch_page_title_cmd`）；
   * 返回 null → 保持 `[](url)` 占位。测试可注入 mock。
   */
  fetchPageTitle?: (url: string) => Promise<string | null>;
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


/** 粘贴进来的 http(s) 链接正则（复制而非手输；已在 [text](url) 内的链接跳过）。 */
const PASTED_URL_RE = /https?:\/\/[^\s<>"'\u3002\uff0c\uff1b\uff1a\uff01\uff1f]+/gi;

/** 去掉 URL 尾部常见的句子/标点。 */
function trimUrlTrailing(raw: string): string {
  let url = raw;
  while (
    url.length > 0 &&
    /[.,;:!?'"\uff09\uff3d\uff1e\}]$/.test(url[url.length - 1])
  ) {
    url = url.slice(0, -1);
  }
  return url;
}

/** 默认标题获取：走 Tauri 后端命令（Rust 侧抓取，规避 WebView CORS）。 */
async function fetchPageTitleDefault(url: string): Promise<string | null> {
  try {
    return await invoke<string | null>("fetch_page_title_cmd", { url });
  } catch {
    return null;
  }
}

/**
 * 粘贴链接自动转 Markdown 链接：
 * 1) 先把粘贴进来的 http(s) 链接替换为 `[](url)` 占位；
 * 2) 异步请求网页标题后填充为 `[title](url)`（失败则保留占位）。
 * 仅响应粘贴（userEvent "input.paste"），手动输入不触发。
 */
async function handlePastedLinks(update: ViewUpdate, opts: LiveViewOptions): Promise<void> {
  const pasted = update.transactions.some((tr) => tr.isUserEvent("input.paste"));
  if (!pasted || !update.docChanged) return;
  const doc = update.state.doc;
  const found: Array<{ from: number; to: number; url: string }> = [];
  update.changes.iterChanges((_fromA, _toA, fromB, _toB, inserted) => {
    const text = inserted.toString();
    PASTED_URL_RE.lastIndex = 0;
    let m: RegExpExecArray | null;
    while ((m = PASTED_URL_RE.exec(text))) {
      const url = trimUrlTrailing(m[0]);
      if (!url) continue;
      const start = fromB + m.index;
      // 已是 [text](url) 的一部分（前面紧跟 "("）→ 不重复转换
      if (start > 0 && doc.sliceString(start - 1, start) === "(") continue;
      found.push({ from: start, to: start + url.length, url });
    }
  });
  if (!found.length) return;
  const view = update.view;
  view.dispatch({
    changes: found.map((p) => ({ from: p.from, to: p.to, insert: `[](${p.url})` })),
  });
  const fetchTitle = opts.fetchPageTitle ?? fetchPageTitleDefault;
  // 占位替换会改变后续链接的偏移（每个占位比裸链接多 4 字符），
  // 先累计偏移得到每个占位的真实位置，再从右往左逐个填充：
  // 靠右插入标题不会改变左侧占位的位置。
  let shift = 0;
  const targets = found.map((p) => {
    const placeholderFrom = p.from + shift;
    shift += 4; // "[](" 与 ")" 合计比裸链接多出的字符数
    return { from: placeholderFrom, url: p.url };
  });
  targets.sort((a, b) => b.from - a.from);
  for (const p of targets) {
    await fillLinkTitle(view, p.from, p.url, fetchTitle);
  }
}

/** 标题就绪后填充 `[title](url)`；期间用户改动过该占位则放弃填充。 */
async function fillLinkTitle(
  view: EditorView,
  from: number,
  url: string,
  fetchTitle: (url: string) => Promise<string | null>,
): Promise<void> {
  let title: string | null = null;
  try {
    title = await fetchTitle(url);
  } catch {
    title = null;
  }
  if (!title || !view.dom.isConnected) return;
  const expect = `[](${url})`;
  if (view.state.doc.sliceString(from, from + expect.length) !== expect) return;
  try {
    view.dispatch({ changes: { from, to: from + 2, insert: `[${title}]` } });
  } catch {
    // 编辑器已销毁等竞态：忽略
  }
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
        {
          key: "Backspace",
          run: (view) => {
            // 任务块/已完成块整行删除：第一次退格删掉整行标签内容（留空行），
            // 第二次退格由默认行为并掉空行——两次即可删除整个块，不会逐字符撕。
            if (deleteBlockLineBeforeCursor(view)) return true;
            return runLiveTransform(view, buildBackspaceTransaction);
          },
        },
        { key: "Mod-b", run: (view) => runFormat(view, "**", "**", "input.format.bold") },
        { key: "Mod-i", run: (view) => runFormat(view, "*", "*", "input.format.italic") },
        { key: "Mod-Shift-x", run: (view) => runFormat(view, "~~", "~~", "input.format.strike") },
        indentWithTab,
        { key: "Mod-s", run: () => { opts.onSave(); return true; } },
      ])),
      EditorView.lineWrapping,
      liveTodoBlocksField.init(() => opts.todoBlocks ?? []),
      liveUiStateField.init(() => ({ ui: opts.ui ?? DEFAULT_BLOCK_UI, uiKey: opts.uiKey ?? "" })),
      liveBlockDecorationsField,
      // Todo/已完成块的原子区间：光标进不去 → 点击不再选中原始终端文本
      liveAtomicRangesField,
      liveDecorationsPlugin,
      EditorView.updateListener.of((u) => {
        if (u.docChanged) {
          const doc = u.state.doc.toString();
          opts.onDocChange(doc);
        }
        if (u.docChanged || u.selectionSet) reportSlash(u.view, (query, from, to, anchor) => opts.onSlash?.(query, from, to, anchor), () => opts.onSlashClose?.());
        void handlePastedLinks(u, opts);
      }),
      fontSizeCompartment.of(fontSizeTheme(opts.fontSize)),
      fontFamilyCompartment.of(fontFamilyTheme(opts.fontFamily ?? "Microsoft YaHei")),
      lightTheme,
    ],
  });
  const view = new EditorView({ state, parent });
  view.dom.addEventListener("click", (event) => {
    const target = event.target as HTMLElement | null;
    // 折叠/子任务/已完成折叠箭头优先于「点卡片开编辑窗」：
    // 点箭头只切换显隐，不弹 Todo 窗口。
    const fold = target?.closest<HTMLElement>("[data-fold]");
    if (fold?.dataset.fold) {
      event.stopPropagation();
      opts.onBlockUiAction?.("foldCard", fold.dataset.fold);
      return;
    }
    const caret = target?.closest<HTMLElement>("[data-caret]");
    if (caret?.dataset.caret) {
      event.stopPropagation();
      opts.onBlockUiAction?.("foldSub", caret.dataset.caret);
      return;
    }
    const doneFold = target?.closest<HTMLElement>("[data-donefold]");
    if (doneFold && doneFold.dataset.donefold !== undefined) {
      event.stopPropagation();
      opts.onBlockUiAction?.("doneFold", doneFold.dataset.donefold ?? "");
      return;
    }
    const card = target?.closest<HTMLElement>(".todo-block-card");
    const id = card?.dataset.todoId;
    if (id) opts.onTodoOpen?.(id);
  });
  // 已完成任务卡：来源下拉切换
  view.dom.addEventListener("change", (event) => {
    const select = event.target as HTMLSelectElement | null;
    if (!select || !select.classList.contains("sd-source")) return;
    event.stopPropagation();
    opts.onDoneSource?.(select.value, select.dataset.sdKey ?? "");
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

/** 外部内容更新 → 同步进编辑器（最小差异替换，保留光标；组词期间跳过）。 */
export function setLiveDoc(view: EditorView, doc: string): void {
  applyExternalDoc(view, doc);
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

/** 用户折叠状态变化后刷新卡片渲染（不触碰光标与文档）。 */
export function setLiveUiStateInView(view: EditorView, ui: BlockUiState, uiKey: string): void {
  view.dispatch({ effects: setLiveUiState.of({ ui, uiKey }) });
}

const BLOCK_LINE_RE = new RegExp("^<(todo-block|show-done)\\b");

/**
 * Backspace 整行删除任务块/已完成块标签：
 *  - 光标在该块行的行尾，或位于其下一空行行首（原子区间把光标推到这里）→
 *    一次退格删除整行内容，留下一个空行；
 *  - 第二次退格由默认行为并掉该空行——两次即可完整删除一个块。
 * 其余情况返回 false 交给后续键位处理。
 */
function deleteBlockLineBeforeCursor(view: EditorView): boolean {
  const { state } = view;
  const selection = state.selection.main;
  if (!selection.empty) return false;
  const pos = selection.head;
  const line = state.doc.lineAt(pos);
  let targetLine: import("@codemirror/state").Line | null = null;
  if (BLOCK_LINE_RE.test(line.text.trim())) {
    // 光标就在块行内（行尾）：删本行
    targetLine = line;
  } else if (pos === line.from && line.number > 1) {
    // 光标在行首且上一行是块行：删上一行（等价于从下一行往上删）
    const prev = state.doc.line(line.number - 1);
    if (BLOCK_LINE_RE.test(prev.text.trim())) targetLine = prev;
  }
  if (!targetLine) return false;
  // 删除整行内容后，光标放在「下一行行首」：这样第二次退格由默认行为
  // 并掉残留空行（两次退格完整删除一个块）。块是最后一行时留在原地。
  const hasNextLine = targetLine.to < state.doc.length;
  view.dispatch({
    changes: { from: targetLine.from, to: targetLine.to },
    selection: { anchor: hasNextLine ? targetLine.from + 1 : targetLine.from },
    userEvent: "delete.backward",
  });
  return true;
}
