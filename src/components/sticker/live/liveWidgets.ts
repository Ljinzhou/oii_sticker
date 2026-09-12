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
  constructor(readonly source: string) {
    super();
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

  ignoreEvent() {
    return false;
  }

  /** 单元格编辑与列宽拖拽（均在 DOM 事件里通过 DOM 反查编辑器，避免持有 stale view）。 */
  private enableEditing(wrapper: HTMLElement, table: HTMLTableElement): void {
    const widget = this;
    for (const [rowIndex, row] of Array.from(table.rows).entries()) {
      for (const [columnIndex, cell] of Array.from(row.cells).entries()) {
        cell.dataset.row = String(rowIndex);
        cell.dataset.col = String(columnIndex);
        cell.setAttribute("contenteditable", "true");
        cell.spellcheck = false;

        let timer = 0;
        const commit = () => {
          if (timer) window.clearTimeout(timer);
          timer = 0;
          const view = viewFromDOM(wrapper);
          if (!view || !wrapper.isConnected) return;
          const pos = widgetPos(view, wrapper);
          if (pos < 0) return;
          const edit = setTableCell(
            view.state.doc.toString(),
            pos + 1,
            rowIndex,
            columnIndex,
            cellMarkdown(cell),
          );
          if (!edit) return;
          view.dispatch({
            changes: { from: edit.from, to: edit.to, insert: edit.insert },
            userEvent: "input.table",
          });
        };

        cell.addEventListener("focus", () => {
          editingTable = widget;
          editingCell = { commit };
          // 选区移到表格边界：工具条据此显示，且不抢走单元格焦点
          const view = viewFromDOM(wrapper);
          const pos = view ? widgetPos(view, wrapper) : -1;
          if (view && pos >= 0 && view.state.selection.main.head !== pos) {
            view.dispatch({ selection: { anchor: pos } });
          }
        });
        cell.addEventListener("input", () => {
          if (timer) window.clearTimeout(timer);
          timer = window.setTimeout(commit, 300);
        });
        cell.addEventListener("blur", () => {
          commit();
          if (editingTable === widget) editingTable = null;
          editingCell = null;
          // 让 DOM 与源码重新对齐（格式化、其它窗口的改动等）
          const view = viewFromDOM(wrapper);
          if (view && view.dom.isConnected) {
            view.dispatch({ effects: refreshLivePreview.of(null) });
          }
        });
        cell.addEventListener("keydown", (event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            cell.blur();
          }
        });
        cell.addEventListener("paste", (event) => {
          // 只接收纯文本：外部富文本会带样式进来，无法用 Markdown 表达
          const text = event.clipboardData?.getData("text/plain");
          if (text === undefined) return;
          event.preventDefault();
          const selection = window.getSelection();
          if (!selection) return;
          selection.deleteFromDocument();
          selection.getRangeAt(0).insertNode(document.createTextNode(text.replace(/\s*\n+\s*/g, " ")));
          selection.collapseToEnd();
        });
      }
    }
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
