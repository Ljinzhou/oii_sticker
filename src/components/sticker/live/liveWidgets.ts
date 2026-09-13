import { EditorView, WidgetType } from "@codemirror/view";
import type { TransactionSpec } from "@codemirror/state";
import { renderMarkdown } from "../../../utils/markdown";
import { htmlToMarkdown, renderMarkdownEditable } from "../../../utils/markdown-editable";
import { DEFAULT_BLOCK_UI, type BlockUiState } from "../../../utils/block-ui";
import {
  alignMarker,
  columnAligns,
  columnWidths,
  setColumnWidth,
  setTableCell,
  type TableCellFormat,
  type TableCellRange,
} from "./liveTableEdits";
import { buildTableRangeTransaction } from "./liveTransforms";
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
/** 当前编辑框的提交函数（供工具条动作、保存前落盘）。 */
let editingCellRef: { commit: () => void } | null = null;

/** 结束表格编辑态（工具条动作等外部改动前调用）：先把未提交的输入落盘。 */
export function endTableEditing(): void {
  const cell = editingCellRef;
  editingTable = null;
  editingCellRef = null;
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

/* ── 单元格选区（表格内的「选单元格」而非「选文字」） ── */

export interface TableCellPos {
  row: number;
  column: number;
}

/** 当前正在交互的单元格选区：anchor = 起点（点击/聚焦的格），head = 终点（拖拽经过的格）。 */
export interface ActiveTableCell {
  /** widget 根元素（.live-table-block）。 */
  host: HTMLElement;
  table: HTMLTableElement;
  anchor: TableCellPos;
  head: TableCellPos;
  /** true = 区域选择（拖拽 / Shift+点击，可多格）；false = 单格光标编辑。 */
  multi: boolean;
}

let activeCell: ActiveTableCell | null = null;

/** 当前单元格选区；widget 已从文档移除时自动失效（工具条据此定位动作目标）。 */
export function getActiveTableCell(): ActiveTableCell | null {
  if (activeCell && !activeCell.host.isConnected) activeCell = null;
  return activeCell;
}

/** 单元格选区的行列范围。 */
export function tableCellRange(state: ActiveTableCell): TableCellRange {
  return {
    fromRow: Math.min(state.anchor.row, state.head.row),
    toRow: Math.max(state.anchor.row, state.head.row),
    fromColumn: Math.min(state.anchor.column, state.head.column),
    toColumn: Math.max(state.anchor.column, state.head.column),
  };
}

/** 当前选区的行列范围（无选区返回 null）。 */
export function activeTableCellRange(): TableCellRange | null {
  const state = getActiveTableCell();
  return state ? tableCellRange(state) : null;
}

/** 清空单元格选区（点击表格外 / Esc / 表格被移除）。 */
export function clearActiveTableCell(): void {
  const state = activeCell;
  activeCell = null;
  if (state?.host.isConnected) paintCellSelection(state.host, null);
}

/** 高亮区域内单元格（range 为 null 时清除高亮）。 */
function paintCellSelection(host: HTMLElement, range: TableCellRange | null): void {
  for (const cell of Array.from(host.querySelectorAll<HTMLTableCellElement>("td, th"))) {
    const row = Number(cell.dataset.row ?? 0);
    const column = Number(cell.dataset.col ?? 0);
    const selected = !!range
      && row >= range.fromRow
      && row <= range.toRow
      && column >= range.fromColumn
      && column <= range.toColumn;
    cell.classList.toggle("is-cell-selected", selected);
  }
}

/** 取表格内指定行列的单元格（越界夹取到最近可聚焦单元格 → 结构变化后仍能落点）。 */
function cellAt(table: HTMLTableElement, row: number, column: number): HTMLTableCellElement | null {
  const rows = table.rows;
  if (!rows.length) return null;
  const rowEl = rows[Math.min(Math.max(row, 0), rows.length - 1)];
  if (!rowEl) return null;
  const cell = rowEl.cells[Math.min(Math.max(column, 0), rowEl.cells.length - 1)];
  return (cell as HTMLTableCellElement | undefined) ?? null;
}

/** widget 在编辑器中的序号：同一次动作前后不变（前面的表格未被改动）→ 用于定位重建后的 DOM。 */
export function tableBlockIndexOf(view: EditorView, host: HTMLElement): number {
  return Array.from(view.dom.querySelectorAll(".live-table-block")).indexOf(host);
}

/** 表格被源码动作改写后 widget DOM 会重建：按序号找回新 DOM，恢复单元格选区与焦点。
 *  DOM 尚未重建时顺延到下一帧（CodeMirror 更新与渲染时序差异）。 */
export function restoreTableCellSelection(
  view: EditorView,
  blockIndex: number,
  range: TableCellRange,
  multi: boolean,
  tries = 2,
): void {
  if (!view.dom.isConnected || blockIndex < 0) return;
  const host = view.dom.querySelectorAll<HTMLElement>(".live-table-block")[blockIndex];
  const table = host?.querySelector<HTMLTableElement>("table") ?? null;
  if (!host || !table) {
    if (tries > 0) requestAnimationFrame(() => restoreTableCellSelection(view, blockIndex, range, multi, tries - 1));
    return;
  }
  const anchorCell = cellAt(table, range.fromRow, range.fromColumn);
  const headCell = cellAt(table, range.toRow, range.toColumn);
  if (!anchorCell || !headCell) return;
  const state: ActiveTableCell = {
    host,
    table,
    anchor: { row: Number(anchorCell.dataset.row ?? 0), column: Number(anchorCell.dataset.col ?? 0) },
    head: { row: Number(headCell.dataset.row ?? 0), column: Number(headCell.dataset.col ?? 0) },
    multi,
  };
  activeCell = state;
  paintCellSelection(host, multi ? tableCellRange(state) : null);
  anchorCell.focus();
}

/** 是否具备真实布局（jsdom 无布局：elementFromPoint 不可用，退回事件目标）。 */
function hasLayout(): boolean {
  return typeof document.elementFromPoint === "function"
    && typeof document.body?.getBoundingClientRect === "function"
    && document.body.getBoundingClientRect().height > 0;
}

/** 鼠标位置 → 单元格（拖拽选中文字时浏览器会把 mousemove 重定向到起点，需按坐标反查）。 */
function cellFromPoint(node: Node | null, clientX: number, clientY: number): HTMLTableCellElement | null {
  const hit: Element | null = node?.nodeType === Node.ELEMENT_NODE
    ? (node as Element)
    : node?.parentElement ?? null;
  if (hasLayout()) {
    try {
      return (document.elementFromPoint(clientX, clientY) ?? hit)?.closest<HTMLTableCellElement>("td, th") ?? null;
    } catch {
      // 无布局环境：忽略
    }
  }
  return hit?.closest<HTMLTableCellElement>("td, th") ?? null;
}

/** 单元格里是否已有文字选区（有 → 交浏览器原生加粗/斜体，保留插入点与撤销）。 */
function cellTextSelection(cell: HTMLElement): Selection | null {
  const selection = document.getSelection();
  if (!selection || selection.isCollapsed || selection.rangeCount === 0) return null;
  const node = selection.anchorNode;
  return node && cell.contains(node) ? selection : null;
}

/** 单元格是否落在选区内。 */
function cellInRange(range: TableCellRange, cell: TableCellPos): boolean {
  return cell.row >= range.fromRow
    && cell.row <= range.toRow
    && cell.column >= range.fromColumn
    && cell.column <= range.toColumn;
}

/** 复制用单元格文本：去掉列宽手柄等非内容节点后取 Markdown。 */
function copyCellMarkdown(cell: HTMLTableCellElement): string {
  const clone = cell.cloneNode(true) as HTMLElement;
  clone.querySelectorAll(".tbl-col-resize").forEach((node) => node.remove());
  return cellMarkdown(clone);
}

/** 选区 → Markdown 表格源码（区域首行作表头；列对齐沿用源码分隔行）。 */
function tableRangeMarkdown(state: ActiveTableCell, source: string): string {
  const range = tableCellRange(state);
  const aligns = columnAligns(source, 0).slice(range.fromColumn, range.toColumn + 1);
  const cells = new Map<string, HTMLTableCellElement>();
  for (const cell of Array.from(state.table.querySelectorAll<HTMLTableCellElement>("td, th"))) {
    cells.set(`${cell.dataset.row ?? 0}:${cell.dataset.col ?? 0}`, cell);
  }
  const lines: string[] = [];
  for (let row = range.fromRow; row <= range.toRow; row++) {
    const values: string[] = [];
    for (let column = range.fromColumn; column <= range.toColumn; column++) {
      const cell = cells.get(`${row}:${column}`);
      values.push(cell ? copyCellMarkdown(cell) : "");
    }
    lines.push(`| ${values.join(" | ")} |`);
    if (row === range.fromRow) {
      // 首行作为表头，紧随一行分隔行才是合法 Markdown 表格
      lines.push(`| ${values.map((_, index) => alignMarker(aligns[index] ?? "default", 3)).join(" | ")} |`);
    }
  }
  return lines.join("\n");
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

  /** 单元格编辑、单元格区域选择与列宽拖拽。
   *  单元格自身就是编辑宿主：widget 根由 CodeMirror 设为 contenteditable="false"，
   *  单元格上的 contenteditable="true" + tabindex 让浏览器把插入点直接放进单元格
   *  （Typora 式直接编辑）。键盘、输入法、退格、插入点全部原生；
   *  事件目标落在 widget 内，编辑器因 ignoreEvent 不会插手。
   *  跨格拖拽 / Shift+点击 → 转为「选单元格」（区域高亮，可批量格式化 / 复制 / 删行列）。 */
  private enableEditing(wrapper: HTMLElement, table: HTMLTableElement): void {
    const widget = this;
    let editingDom: HTMLTableCellElement | null = null;
    let originalHtml = "";
    /** 拖拽选择的起点（mousedown 记录，mouseup 结束）。 */
    let dragAnchor: HTMLTableCellElement | null = null;
    /** 已从「格内选字」切到「选单元格」：接管这次拖拽，禁止原生文本选择扩张。 */
    let cellDragging = false;

    const pos = (cell: HTMLTableCellElement): TableCellPos => ({
      row: Number(cell.dataset.row ?? 0),
      column: Number(cell.dataset.col ?? 0),
    });

    /** 本 widget 自己的选区（不同表格的选区互斥）。 */
    const selection = (): ActiveTableCell | null =>
      activeCell && activeCell.host === wrapper ? activeCell : null;

    const select = (anchor: TableCellPos, head: TableCellPos, multi: boolean) => {
      if (activeCell && activeCell.host !== wrapper && activeCell.host.isConnected) {
        paintCellSelection(activeCell.host, null); // 另一张表的选择随之取消
      }
      const state: ActiveTableCell = { host: wrapper, table, anchor, head, multi };
      activeCell = state;
      paintCellSelection(wrapper, multi ? tableCellRange(state) : null);
    };

    /** 单元格内容 → Markdown 源码（保留 **、` 等标记）。 */
    const commitCell = (cell: HTMLTableCellElement) => {
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

    /** 提交未落盘输入 → 单事务改写源码 → 恢复单元格选区与焦点（DOM 会随之重建）。 */
    const runSourceAction = (
      build: (
        text: string,
        tablePos: number,
        range: TableCellRange | null,
      ) => { spec: TransactionSpec; range: TableCellRange } | null,
    ): boolean => {
      const view = viewFromDOM(wrapper);
      if (!view) return false;
      const tablePos = widgetPos(view, wrapper);
      if (tablePos < 0) return false;
      const current = selection();
      const range = current ? tableCellRange(current) : null;
      const multi = current?.multi ?? false;
      // 先记下 widget 序号：源码更新后 DOM 重建，本表之前的表格未被改动 → 序号不变
      const index = tableBlockIndexOf(view, wrapper);
      endTableEditing(); // 未提交的输入先落盘
      const built = build(view.state.doc.toString(), tablePos + 1, range);
      if (!built) return false;
      view.dispatch(built.spec);
      restoreTableCellSelection(view, index, built.range, multi);
      return true;
    };

    /** 加粗 / 斜体 / 删除线：格内已选字 → 交浏览器原生（turndown 回写标记）；
     *  否则作用于「选中的单元格」整体（源码级，可撤销）。 */
    const handleFormatKey = (cell: HTMLTableCellElement, event: KeyboardEvent): boolean => {
      const mod = event.ctrlKey || event.metaKey;
      if (!mod || event.altKey) return false;
      const key = event.key.toLowerCase();
      const format: TableCellFormat | null = key === "b"
        ? "bold"
        : key === "i"
          ? "italic"
          : key === "x" && event.shiftKey
            ? "strike"
            : null;
      if (!format) return false;
      if (cellTextSelection(cell)) {
        // 格内选字：用浏览器编辑命令（turndown 回写 ** / * / ~~），保留插入点与撤销
        const command = format === "bold" ? "bold" : format === "italic" ? "italic" : "strikeThrough";
        if (typeof document.execCommand !== "function") return false;
        event.preventDefault();
        document.execCommand(command);
        return true;
      }
      event.preventDefault(); // 源码由表格层维护，不让浏览器直接改 DOM
      return runSourceAction((text, tablePos, range) =>
        buildTableRangeTransaction(text, tablePos, range, format));
    };

    const enterEditing = (cell: HTMLTableCellElement) => {
      const target = pos(cell);
      const current = selection();
      // 焦点落在当前区域选区内（动作后恢复焦点）→ 保留区域选择；否则收敛为单格
      if (!current?.multi || !cellInRange(tableCellRange(current), target)) {
        select(target, target, false);
      }
      editingDom = cell;
      originalHtml = cell.innerHTML;
      // 编辑期间保持保护：装饰重建时复用本 widget 的 DOM，焦点与插入点不丢
      editingTable = widget;
      editingCellRef = { commit: () => commitCell(cell) };
      cell.classList.add("is-editing");
    };

    const leaveEditing = (cell: HTMLTableCellElement) => {
      if (editingDom !== cell) return;
      commitCell(cell); // 此时仍在编辑保护中 → DOM 不被替换 → 焦点保持
      editingDom = null;
      editingCellRef = null;
      cell.classList.remove("is-editing");
      if (editingTable === widget) editingTable = null;
      // 焦点已离开本表格 → 清掉单元格选区（工具条随之隐藏）
      window.setTimeout(() => {
        const active = document.activeElement as HTMLElement | null;
        if (active?.closest?.(".live-table-block") === wrapper) return;
        if (activeCell?.host === wrapper) clearActiveTableCell();
      }, 0);
    };

    /** Tab / Shift+Tab 在单元格之间移动。 */
    const moveToSibling = (cell: HTMLTableCellElement, direction: 1 | -1) => {
      const cells = Array.from(wrapper.querySelectorAll<HTMLTableCellElement>("td, th"));
      const next = cells[cells.indexOf(cell) + direction];
      if (next) next.focus();
      else cell.blur();
    };

    /** 普通按下 = 单格光标（格内仍可选字）；Shift+按下 = 从锚点扩展单元格选区。 */
    const onCellMouseDown = (cell: HTMLTableCellElement, event: MouseEvent) => {
      if (event.button !== 0) return;
      if ((event.target as HTMLElement | null)?.closest?.(".tbl-col-resize")) return;
      if (event.shiftKey) {
        event.preventDefault(); // 不扩展文本选区，改为扩展单元格选区
        select(selection()?.anchor ?? pos(cell), pos(cell), true);
        return;
      }
      dragAnchor = cell;
      cellDragging = false;
      select(pos(cell), pos(cell), false);
      document.addEventListener("mousemove", onDocumentMove);
      document.addEventListener("mouseup", onDocumentUp);
    };

    /** 拖拽经过其它单元格 → 转为「选单元格」并接管这次拖拽。 */
    const onDocumentMove = (event: MouseEvent) => {
      if (!dragAnchor) return;
      const target = cellFromPoint(event.target as Node | null, event.clientX, event.clientY);
      if (!target || !wrapper.contains(target)) return;
      if (target !== dragAnchor) cellDragging = true;
      if (!cellDragging) return; // 仍在同一格内：保留原生「格内选字」
      event.preventDefault();
      document.getSelection()?.removeAllRanges(); // 去掉拖拽途中的文本选区，只留单元格高亮
      select(pos(dragAnchor), pos(target), true);
    };

    const onDocumentUp = () => {
      dragAnchor = null;
      cellDragging = false;
      document.removeEventListener("mousemove", onDocumentMove);
      document.removeEventListener("mouseup", onDocumentUp);
    };

    /** 进入「选单元格」阶段后：阻止原生文本选择继续扩张。 */
    const onSelectStart = (event: Event) => {
      if (cellDragging) event.preventDefault();
    };

    /** Ctrl+C：区域选择时复制为 Markdown 表格源码（格内选字仍走原生复制）。 */
    const onCopy = (event: ClipboardEvent) => {
      const current = selection();
      if (!current?.multi) return;
      const text = tableRangeMarkdown(current, this.source);
      if (!text) return;
      event.preventDefault();
      event.clipboardData?.setData("text/plain", text);
    };

    const onCellKeyDown = (cell: HTMLTableCellElement, event: KeyboardEvent) => {
      if (event.isComposing) return; // 输入法组词期间不特殊处理
      if (handleFormatKey(cell, event)) return;
      if (event.key === "Enter") {
        event.preventDefault(); // 单元格内不换行
        commitCell(cell);
      } else if (event.key === "Escape") {
        event.preventDefault();
        if (selection()?.multi) {
          clearActiveTableCell(); // 先退出区域选择，再按一次才是放弃修改
          return;
        }
        cell.innerHTML = originalHtml; // 放弃这次修改
        cell.blur();
      } else if (event.key === "Tab") {
        event.preventDefault();
        moveToSibling(cell, event.shiftKey ? -1 : 1);
      } else if ((event.key === "Backspace" || event.key === "Delete") && selection()?.multi) {
        event.preventDefault();
        runSourceAction((text, tablePos, range) =>
          buildTableRangeTransaction(text, tablePos, range, "cells-clear"));
      }
    };

    for (const [rowIndex, row] of Array.from(table.rows).entries()) {
      for (const [columnIndex, cell] of Array.from(row.cells).entries()) {
        cell.dataset.row = String(rowIndex);
        cell.dataset.col = String(columnIndex);
        cell.setAttribute("contenteditable", "true");
        cell.setAttribute("tabindex", "0");
        cell.spellcheck = false;
        cell.addEventListener("focus", () => enterEditing(cell));
        cell.addEventListener("blur", () => leaveEditing(cell));
        cell.addEventListener("mousedown", (event) => onCellMouseDown(cell, event));
        cell.addEventListener("keydown", (event) => onCellKeyDown(cell, event));
      }
    }
    wrapper.addEventListener("selectstart", onSelectStart);
    wrapper.addEventListener("copy", onCopy);
    this.cleanups.push(() => {
      editingDom = null;
      if (editingTable === widget) {
        editingTable = null;
        editingCellRef = null;
      }
      wrapper.removeEventListener("selectstart", onSelectStart);
      wrapper.removeEventListener("copy", onCopy);
      document.removeEventListener("mousemove", onDocumentMove);
      document.removeEventListener("mouseup", onDocumentUp);
      if (activeCell?.host === wrapper) activeCell = null;
    });

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
