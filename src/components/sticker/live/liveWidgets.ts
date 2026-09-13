import { EditorView, WidgetType } from "@codemirror/view";
import { renderMarkdown } from "../../../utils/markdown";
import { htmlToMarkdown, renderMarkdownEditable } from "../../../utils/markdown-editable";
import { DEFAULT_BLOCK_UI, type BlockUiState } from "../../../utils/block-ui";
import { columnWidths, setColumnWidth, setTableCell } from "./liveTableEdits";
import { refreshLivePreview } from "./liveEffects";
import type { TodoBlock } from "../../../types";

/** 渲染 markdown-it 片段并提取行内 HTML。 */
export function renderFragment(src: string): string {
  const html = renderMarkdownEditable(src);
  const match = /<p>(.*?)<\/p>\s*$/s.exec(html);
  return match ? match[1] : html;
}

/** 将完整 fenced 代码块渲染为语义化代码块。 */
export class CodeBlockWidget extends WidgetType {
  constructor(readonly code: string, readonly language?: string) {
    super();
  }

  eq(other: CodeBlockWidget) {
    return other.code === this.code && other.language === this.language;
  }

  toDOM() {
    const wrapper = document.createElement("div");
    const pre = document.createElement("pre");
    const code = document.createElement("code");
    wrapper.className = "live-code-block";
    if (this.language) code.className = `language-${this.language}`;
    code.textContent = this.code;
    pre.append(code);
    wrapper.append(pre);
    return wrapper;
  }

  ignoreEvent() {
    return false;
  }
}

/** 将 $$...$$ 块替换为 MathJax 输出。 */
export class MathBlockWidget extends WidgetType {
  constructor(readonly source: string, readonly rendererVersion: number) {
    super();
  }

  eq(other: MathBlockWidget) {
    // MathJax is initialized asynchronously.  A block created before it is
    // ready contains fallback Markdown; its DOM must not be reused afterwards.
    return other.source === this.source && other.rendererVersion === this.rendererVersion;
  }

  toDOM() {
    const wrapper = document.createElement("div");
    wrapper.className = "live-math-block";
    wrapper.innerHTML = renderMarkdownEditable(this.source);
    return wrapper;
  }

  ignoreEvent() {
    return false;
  }
}

/** 正在编辑单元格的表格 widget：输入期间不重建 DOM，否则会打断输入与输入法组词。 */
let editingTable: TableBlockWidget | null = null;
/** 当前获得焦点的单元格：连同其"立即提交"函数，供工具条动作前落盘。 */
let editingCell: { commit: () => void } | null = null;

/** 结束表格编辑态（工具条动作等外部改动前调用）：先把未提交的输入落盘。 */
export function endTableEditing(): void {
  const cell = editingCell;
  editingTable = null;
  editingCell = null;
  cell?.commit();
}

/** 通过 DOM 反查持有它的编辑器实例（widget 已挂载）。 */
function viewFromDOM(node: HTMLElement): EditorView | null {
  try {
    return EditorView.findFromDOM(node) ?? null;
  } catch {
    return null;
  }
}

/** 单元格 HTML → Markdown（粗体/行内代码等由 turndown 保真还原，换行折叠为空格）。 */
function cellMarkdown(cell: HTMLElement): string {
  const html = cell.innerHTML.trim();
  if (!html) return "";
  return htmlToMarkdown(html).replace(/\s*\n+\s*/g, " ").trim();
}

/** 单元格在文档中的位置（挂载后取真实位置，避免使用陈旧的构造期偏移）。 */
function widgetPos(view: EditorView, wrapper: HTMLElement): number {
  try {
    return view.posAtDOM(wrapper);
  } catch {
    return -1;
  }
}

/** 用最小 diff 写回全文（整表改动：例如列宽拖拽）。 */
function replaceDoc(view: EditorView, next: string, userEvent: string): void {
  const current = view.state.doc.toString();
  if (current === next) return;
  let from = 0;
  while (from < current.length && from < next.length && current[from] === next[from]) from++;
  let endCurrent = current.length;
  let endNext = next.length;
  while (endCurrent > from && endNext > from && current[endCurrent - 1] === next[endNext - 1]) {
    endCurrent--;
    endNext--;
  }
  view.dispatch({
    changes: { from, to: endCurrent, insert: next.slice(from, endNext) },
    userEvent,
  });
}

/**
 * 将完整 GFM 表格渲染为「可直接编辑」的 HTML 表格。
 * - 表格整体作为一个块级 widget：单元格内文本不再单独走行内 decoration；
 * - 表格永远渲染（光标进入也不回退源码），单元格 contenteditable 直接改字，
 *   输入防抖写回 Markdown 源码；粗体/代码等格式经 turndown 保真回写；
 * - 表头单元格右侧有列宽拖拽手柄：拖动只改分隔行的连字符宽度（持久化到源码）。
 */
export class TableBlockWidget extends WidgetType {
  /** widget 内注册的 document 级监听清理函数（widget 被移除时调用）。 */
  private cleanups: Array<() => void> = [];

  constructor(readonly source: string) {
    super();
  }

  destroy() {
    for (const cleanup of this.cleanups) cleanup();
    this.cleanups = [];
  }

  eq(other: TableBlockWidget) {
    // 编辑期间保持自身 DOM：源码更新（自己的输入）不应重建节点
    if (editingTable === this) return true;
    return other.source === this.source;
  }

  toDOM() {
    const wrapper = document.createElement("div");
    wrapper.className = "live-table-block";
    wrapper.innerHTML = renderMarkdownEditable(this.source);
    const table = wrapper.querySelector("table");
    if (table) {
      applyColumnWidths(table, this.source);
      this.enableEditing(wrapper, table);
    }
    return wrapper;
  }

  /** widget 内的事件由 widget 自己处理。
   *  若交给编辑器（返回 false），mousedown 会被编辑器接管：光标被移到表格前/后并
   *  preventDefault，单元格永远拿不到焦点（表现为「点不进单元格」）。 */
  ignoreEvent(_event: Event) {
    return true;
  }

  /** 声明 widget 内容可编辑：CodeMirror 仅在此为 true 时不给 widget 根节点设置
   *  contenteditable="false"（该属性会压制单元格上的 contenteditable）。 */
  get editable() {
    return true;
  }

  /** 单元格编辑与列宽拖拽。
   *  浏览器把插入点放进单元格（DOM 选区在 td 内），但焦点宿主始终是编辑器正文、
   *  input 事件的目标是编辑器行元素——所以监听只能挂在 document 捕获阶段，
   *  再依据 DOM 选区判定到底编辑的是哪个单元格。 */
  private enableEditing(wrapper: HTMLElement, table: HTMLTableElement): void {
    const widget = this;
    for (const [rowIndex, row] of Array.from(table.rows).entries()) {
      for (const [columnIndex, cell] of Array.from(row.cells).entries()) {
        cell.dataset.row = String(rowIndex);
        cell.dataset.col = String(columnIndex);
        cell.setAttribute("contenteditable", "true");
        cell.spellcheck = false;
      }
    }

    /** 当前 DOM 选区所在的单元格（限本 widget 内）。 */
    const selectionCell = (): HTMLTableCellElement | null => {
      const node = document.getSelection()?.anchorNode ?? null;
      if (!node) return null;
      const element = node.nodeType === Node.ELEMENT_NODE ? (node as HTMLElement) : node.parentElement;
      const cell = element?.closest<HTMLTableCellElement>("td, th") ?? null;
      return cell && wrapper.contains(cell) ? cell : null;
    };

    let timer = 0;
    let edited: HTMLTableCellElement | null = null;

    const commit = (cell: HTMLTableCellElement | null) => {
      if (timer) window.clearTimeout(timer);
      timer = 0;
      if (!cell) return;
      const view = viewFromDOM(wrapper);
      if (!view || !wrapper.isConnected) return;
      const pos = widgetPos(view, wrapper);
      if (pos < 0) return;
      const edit = setTableCell(
        view.state.doc.toString(),
        pos + 1,
        Number(cell.dataset.row ?? 0),
        Number(cell.dataset.col ?? 0),
        cellMarkdown(cell),
      );
      if (!edit) return;
      view.dispatch({
        changes: { from: edit.from, to: edit.to, insert: edit.insert },
        userEvent: "input.table",
      });
    };

    const markEditing = (cell: HTMLTableCellElement) => {
      edited = cell;
      editingTable = widget;
      editingCell = { commit: () => commit(cell) };
    };

    // 输入期间【不写回】：写回会 view.dispatch → 编辑器把 DOM 光标同步出单元格，
    // 用户打第二个字就落到正文里（逐字输入实测：只有第一个字进得了单元格）。
    // 单元格内容交给浏览器原生编辑，等离开单元格时再一次性落盘。
    const onInput = () => {
      const cell = selectionCell();
      if (cell) markEditing(cell);
    };
    // 回车提交（单元格内不换行）；Ctrl/Cmd+S 先把单元格内容落盘再交给保存流程
    const onKeydown = (event: KeyboardEvent) => {
      if (event.isComposing) return;
      const cell = selectionCell();
      if (!cell) return;
      const save = (event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s";
      if (event.key !== "Enter" && !save) return;
      if (event.key === "Enter") event.preventDefault();
      commit(cell);
    };
    // 粘贴只收纯文本（富文本无法用 Markdown 表达）
    const onPaste = (event: ClipboardEvent) => {
      const cell = selectionCell();
      if (!cell) return;
      const text = event.clipboardData?.getData("text/plain");
      if (text === undefined) return;
      event.preventDefault();
      const selection = document.getSelection();
      if (!selection || selection.rangeCount === 0) return;
      const range = selection.getRangeAt(0);
      range.deleteContents();
      const node = document.createTextNode(text.replace(/\s*\n+\s*/g, " "));
      range.insertNode(node);
      range.setStartAfter(node);
      range.collapse(true);
      selection.removeAllRanges();
      selection.addRange(range);
      markEditing(cell);
    };
    // 选区变化：高亮正在编辑的单元格；离开表格则落盘并让 DOM 与源码对齐
    const onSelectionChange = () => {
      const cell = selectionCell();
      wrapper.querySelectorAll("td.is-editing, th.is-editing").forEach((el) => el.classList.remove("is-editing"));
      if (cell) {
        cell.classList.add("is-editing");
        markEditing(cell);
        return;
      }
      if (!edited) return;
      const leaving = edited;
      edited = null;
      commit(leaving);
      if (editingTable === widget) editingTable = null;
      editingCell = null;
      const view = viewFromDOM(wrapper);
      if (view && view.dom.isConnected) view.dispatch({ effects: refreshLivePreview.of(null) });
    };

    document.addEventListener("input", onInput, true);
    document.addEventListener("keydown", onKeydown, true);
    document.addEventListener("paste", onPaste, true);
    document.addEventListener("selectionchange", onSelectionChange);
    this.cleanups.push(
      () => document.removeEventListener("input", onInput, true),
      () => document.removeEventListener("keydown", onKeydown, true),
      () => document.removeEventListener("paste", onPaste, true),
      () => document.removeEventListener("selectionchange", onSelectionChange),
    );

    this.enableColumnResize(wrapper, table);
  }

  /** 列宽拖拽：拖动时只改视觉宽度，松手写回分隔行（持久化）。 */
  private enableColumnResize(wrapper: HTMLElement, table: HTMLTableElement): void {
    const headCells = Array.from(table.rows[0]?.cells ?? []);
    const widths = columnWidths(this.source, 0);
    const cols = Array.from(table.querySelectorAll<HTMLTableColElement>("colgroup col"));
    headCells.forEach((cell, columnIndex) => {
      const handle = document.createElement("span");
      handle.className = "tbl-col-resize";
      handle.dataset.col = String(columnIndex);
      handle.addEventListener("mousedown", (event) => {
        event.preventDefault();
        event.stopPropagation();
        const startX = event.clientX;
        const startWidth = widths[columnIndex] ?? 3;
        const cellWidth = cell.getBoundingClientRect().width || startWidth * 8;
        const perChar = cellWidth / Math.max(1, startWidth);
        let next = startWidth;
        const paint = () => {
          const total = widths.reduce((sum, item, index) => sum + (index === columnIndex ? next : item), 0);
          const col = cols[columnIndex];
          if (col) col.style.width = `${((next / Math.max(1, total)) * 100).toFixed(2)}%`;
        };
        const onMove = (move: MouseEvent) => {
          next = Math.max(3, startWidth + Math.round((move.clientX - startX) / perChar));
          paint();
        };
        const onUp = () => {
          document.removeEventListener("mousemove", onMove);
          document.removeEventListener("mouseup", onUp);
          const view = viewFromDOM(wrapper);
          if (!view) return;
          const pos = widgetPos(view, wrapper);
          if (pos < 0) return;
          const result = setColumnWidth(view.state.doc.toString(), pos + 1, next);
          if (!result) return;
          replaceDoc(view, result.text, "input.table");
        };
        document.addEventListener("mousemove", onMove);
        document.addEventListener("mouseup", onUp);
      });
      cell.append(handle);
    });
  }
}

/** 按分隔行宽度给每列设定显示宽度（table-layout: fixed，保证拖拽与源码一致）。 */
function applyColumnWidths(table: HTMLTableElement, source: string): void {
  const widths = columnWidths(source, 0);
  if (!widths.length) return;
  const total = widths.reduce((sum, width) => sum + width, 0) || 1;
  const group = document.createElement("colgroup");
  for (const width of widths) {
    const col = document.createElement("col");
    col.style.width = `${((width / total) * 100).toFixed(2)}%`;
    group.append(col);
  }
  table.prepend(group);
  table.style.tableLayout = "fixed";
}

/** 将受控 Todo 标签替换为与展示模式一致的任务卡片（含用户折叠状态）。 */
export class TodoBlockWidget extends WidgetType {
  constructor(
    readonly source: string,
    readonly todoBlocks: TodoBlock[],
    readonly ui: BlockUiState = DEFAULT_BLOCK_UI,
    readonly uiKey = "",
  ) {
    super();
  }

  eq(other: TodoBlockWidget) {
    return other.source === this.source
      && other.uiKey === this.uiKey
      && JSON.stringify(other.todoBlocks) === JSON.stringify(this.todoBlocks)
      && JSON.stringify(other.ui) === JSON.stringify(this.ui);
  }

  toDOM() {
    const wrapper = document.createElement("div");
    wrapper.className = "live-todo-block";
    wrapper.innerHTML = renderMarkdown(this.source, this.todoBlocks, false, this.ui, this.uiKey);
    return wrapper;
  }

  ignoreEvent() {
    return false;
  }
}

/** 将“已完成”功能标签替换为已完成任务列表（含来源选择与完成时刻）。 */
export class DoneBlockWidget extends WidgetType {
  constructor(
    readonly source: string,
    readonly todoBlocks: TodoBlock[],
    readonly ui: BlockUiState = DEFAULT_BLOCK_UI,
    readonly uiKey = "",
  ) {
    super();
  }

  eq(other: DoneBlockWidget) {
    return other.source === this.source
      && other.uiKey === this.uiKey
      && JSON.stringify(other.todoBlocks) === JSON.stringify(this.todoBlocks)
      && JSON.stringify(other.ui) === JSON.stringify(this.ui);
  }

  toDOM() {
    const wrapper = document.createElement("div");
    wrapper.className = "live-done-block";
    wrapper.innerHTML = renderMarkdown(this.source, this.todoBlocks, false, this.ui, this.uiKey);
    return wrapper;
  }

  ignoreEvent() {
    return false;
  }
}

/** 将行内 Markdown 片段替换为渲染 DOM。 */
export class InlineRenderWidget extends WidgetType {
  constructor(readonly html: string, readonly cls: string) {
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

/** 任务 checkbox，持有稳定的 Markdown 源范围。 */
export class TaskCheckboxWidget extends WidgetType {
  constructor(
    readonly checked: boolean,
    readonly view: EditorView,
    readonly from: number,
    readonly to: number,
  ) {
    super();
  }

  eq(other: TaskCheckboxWidget) {
    return other.checked === this.checked && other.from === this.from && other.to === this.to;
  }

  toDOM() {
    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.className = "live-task-checkbox";
    checkbox.checked = this.checked;
    checkbox.setAttribute("aria-label", this.checked ? "取消任务" : "完成任务");
    checkbox.addEventListener("click", (event) => {
      event.preventDefault();
      this.view.dispatch({
        changes: {
          from: this.from,
          to: this.to,
          insert: this.checked ? "[ ]" : "[x]",
        },
      });
      this.view.focus();
    });
    return checkbox;
  }

  ignoreEvent() {
    return false;
  }
}

/** 列表 marker widget。 */
export class ListMarkWidget extends WidgetType {
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
export class HrWidget extends WidgetType {
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

/** 标题 marker widget。 */
export class HeadingMarkWidget extends WidgetType {
  eq() {
    return true;
  }

  toDOM() {
    const span = document.createElement("span");
    span.className = "live-heading-mark";
    return span;
  }

  ignoreEvent() {
    return true;
  }
}

/** 引用 marker widget。 */
export class QuoteMarkWidget extends WidgetType {
  eq() {
    return true;
  }

  toDOM() {
    const span = document.createElement("span");
    span.className = "live-quote-mark";
    span.textContent = " ";
    return span;
  }

  ignoreEvent() {
    return true;
  }
}
