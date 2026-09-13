// Markdown 表格的结构化编辑内核（表格工具窗的操作层）。
// 设计要点：
// - 只改必要的行/格，其余内容保持原样（整表重排会产生大 diff 并抹掉用户格式）；
// - 全部为纯函数：输入原文 + 目标位置 + 单元格区域，输出新文本 + 新光标位置 + 新选区，
//   由 liveTransforms 包成最小 CodeMirror Transaction；
// - 所有写操作都保证表格结构合法（表头 + 分隔行始终存在、分隔行始终在第二行）；
// - 操作目标统一是「单元格区域」（单格 = 行列两端相同）：选多格时批量改对齐 / 加粗 / 删行删列。
import { parseTableAt } from "./liveTables";

/** 列对齐；default = 无冒号（Markdown 默认左对齐）。 */
export type TableAlign = "default" | "left" | "center" | "right";

/** 单元格格式（工具条 B / I / S）。 */
export type TableCellFormat = "bold" | "italic" | "strike";

/** 操作目标：表格内的单元格区域，行列均含端点（单格时 from == to）。 */
export interface TableCellRange {
  fromRow: number;
  toRow: number;
  fromColumn: number;
  toColumn: number;
}

export interface TableEditContext {
  /** 表格首行（表头行）的文档行号。 */
  startLine: number;
  /** 表格末行的文档行号。 */
  endLine: number;
  /** 分隔行的文档行号。 */
  delimiterLine: number;
  /** 选区锚点所在行（0 = 表头行）。 */
  row: number;
  /** 选区锚点所在列。 */
  column: number;
  /** 含表头的总行数。 */
  rowCount: number;
  columnCount: number;
  /** 每列对齐方式（表头列数）。 */
  aligns: TableAlign[];
}

export interface TableEditResult {
  text: string;
  cursor: number;
}

/** 表格编辑结果 + 编辑后应保持选中的单元格区域（UI 依此恢复高亮与焦点）。 */
export interface TableRangeEditResult extends TableEditResult {
  range: TableCellRange;
}

/** 单格选区。 */
export function singleCellRange(row: number, column: number): TableCellRange {
  return { fromRow: row, toRow: row, fromColumn: column, toColumn: column };
}

/** 选区归一化（拖拽方向任意）。 */
export function normalizeRange(range: TableCellRange): TableCellRange {
  return {
    fromRow: Math.min(range.fromRow, range.toRow),
    toRow: Math.max(range.fromRow, range.toRow),
    fromColumn: Math.min(range.fromColumn, range.toColumn),
    toColumn: Math.max(range.fromColumn, range.toColumn),
  };
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), Math.max(min, max));
}

/** 选区按表格实际行列夹取（DOM 选区可能因源码变化而越界）。 */
function clampRange(range: TableCellRange, ctx: TableEditContext): TableCellRange {
  const normalized = normalizeRange(range);
  return {
    fromRow: clamp(normalized.fromRow, 0, ctx.rowCount - 1),
    toRow: clamp(normalized.toRow, 0, ctx.rowCount - 1),
    fromColumn: clamp(normalized.fromColumn, 0, ctx.columnCount - 1),
    toColumn: clamp(normalized.toColumn, 0, ctx.columnCount - 1),
  };
}

/** 拆分表格行的单元格（保留 `\|` 转义，两端 trim）。 */
function splitRow(line: string): string[] {
  const inner = line.trim().replace(/^\|/, "").replace(/\|$/, "");
  const cells: string[] = [];
  let current = "";
  for (let index = 0; index < inner.length; index++) {
    const char = inner[index];
    if (char === "\\" && inner[index + 1] === "|") {
      current += "\\|";
      index++;
      continue;
    }
    if (char === "|") {
      cells.push(current.trim());
      current = "";
      continue;
    }
    current += char;
  }
  cells.push(current.trim());
  return cells;
}

/** 组装表格行（与 Tab 补行的既有格式一致：`| a | b |`）。 */
function joinRow(cells: string[]): string {
  return `| ${cells.join(" | ")} |`;
}

function parseAlignCell(cell: string): TableAlign {
  const text = cell.trim();
  const left = text.startsWith(":");
  const right = text.endsWith(":");
  if (left && right) return "center";
  if (right) return "right";
  if (left) return "left";
  return "default";
}

/** 生成分隔行单元格内容；宽度尽量沿用原宽度，连字符最少 3 个（GFM 要求）。 */
export function alignMarker(align: TableAlign, width: number): string {
  switch (align) {
    case "center":
      return `:${"-".repeat(Math.max(3, width - 2))}:`;
    case "right":
      return `${"-".repeat(Math.max(3, width - 1))}:`;
    case "left":
      return `:${"-".repeat(Math.max(3, width - 1))}`;
    default:
      return "-".repeat(Math.max(3, width));
  }
}

/** 单元格内的光标落点：该单元格内容起始处（空单元格落在占位空格之间）。 */
function rowCellOffset(line: string, column: number): number {
  const pipes: number[] = [];
  for (let index = 0; index < line.length; index++) {
    if (line[index] === "|" && line[index - 1] !== "\\") pipes.push(index);
  }
  const leading = line.trimStart().startsWith("|");
  const pipe = pipes[leading ? column : column - 1];
  if (pipe === undefined) return line.length;
  const next = pipe + 1;
  return line[next] === " " ? next + 1 : next;
}

/** 单元格内容区间（去掉两侧空格；空单元格退化为插入点）。 */
function cellBounds(line: string, column: number): { from: number; to: number } | null {
  const pipes: number[] = [];
  for (let index = 0; index < line.length; index++) {
    if (line[index] === "|" && line[index - 1] !== "\\") pipes.push(index);
  }
  const leading = line.trimStart().startsWith("|");
  const index = leading ? column : column - 1;
  const start = pipes[index];
  const end = pipes[index + 1];
  if (start === undefined || end === undefined) return null;
  // 区间含两侧空格：写回时统一补标准空格，避免出现 `|a|` 这类紧凑写法
  return { from: start + 1, to: end };
}

/** 一行内每个单元格的内容区间（trim 后；支持 `\|` 转义）。 */
function rowValueSpans(line: string): Array<{ from: number; to: number }> {
  const pipes: number[] = [];
  for (let index = 0; index < line.length; index++) {
    if (line[index] === "|" && line[index - 1] !== "\\") pipes.push(index);
  }
  const leading = line.trimStart().startsWith("|");
  const first = leading ? 0 : -1;
  const last = line.trimEnd().endsWith("|") ? pipes.length - 1 : pipes.length;
  const spans: Array<{ from: number; to: number }> = [];
  for (let index = first; index < last; index++) {
    const start = index < 0 ? 0 : pipes[index] + 1;
    const end = index + 1 < pipes.length ? pipes[index + 1] : line.length;
    const raw = line.slice(start, end);
    const leadingSpaces = raw.length - raw.trimStart().length;
    spans.push({ from: start + leadingSpaces, to: start + raw.trimEnd().length });
  }
  return spans;
}

/** 解析光标所在表格的结构与光标行列；不在表格内返回 null。 */
export function resolveTableEdit(text: string, position: number): TableEditContext | null {
  const model = parseTableAt(text, position);
  if (!model) return null;
  const lines = text.split("\n");
  const header = splitRow(lines[model.startLine] ?? "");
  const aligns = splitRow(lines[model.delimiterLine] ?? "").map(parseAlignCell);
  const docLine = lines.findIndex((line, index) => {
    const from = lines.slice(0, index).reduce((sum, item) => sum + item.length + 1, 0);
    return position >= from && position <= from + line.length;
  });
  const cell = model.cells.find((item) => position >= item.from && position <= item.to);
  // 光标落在分隔行 / 行首竖线上时没有单元格：按行归入表头或对应数据行。
  const row = cell
    ? cell.row
    : docLine <= model.delimiterLine
      ? 0
      : Math.max(1, docLine - model.startLine - 1);
  return {
    startLine: model.startLine,
    endLine: model.endLine,
    delimiterLine: model.delimiterLine,
    row,
    column: cell?.column ?? 0,
    rowCount: model.endLine - model.startLine,
    columnCount: header.length,
    aligns,
  };
}

/** 表格始终渲染时源码行被折叠，光标只能停在表格边界：
 *  向前后各探一步再判定，保证工具条仍能识别"光标就在这张表上"。 */
export function resolveTableAround(text: string, position: number): TableEditContext | null {
  for (const probe of [position, position - 1, position + 1]) {
    if (probe < 0 || probe > text.length) continue;
    const ctx = resolveTableEdit(text, probe);
    if (ctx) return ctx;
  }
  return null;
}

/** 每列对齐方式（表头列数）。 */
export function columnAligns(text: string, position: number): TableAlign[] {
  const ctx = resolveTableEdit(text, position);
  if (!ctx) return [];
  return splitRow(text.split("\n")[ctx.delimiterLine] ?? "").map(parseAlignCell);
}

/** 表格内"视觉行" → 表格行数组下标（含分隔行）。 */
function inlineRowIndex(row: number): number {
  return row === 0 ? 0 : row + 1;
}

interface RangeTarget {
  row: number;
  column: number;
  /** 编辑后应保持选中的区域（默认 = 目标单元格）；批量操作保持原选区。 */
  range?: TableCellRange;
}

type TableMutator = (
  rows: string[],
  ctx: TableEditContext,
  range: TableCellRange,
) => RangeTarget | null;

/** 统一入口：解析表格 → 在表格行数组上执行改动 → 算回绝对光标位置与新选区。
 *  range 为空时按 position 解析出的单元格（光标路径）。 */
function runTableEdit(
  text: string,
  position: number,
  mutate: TableMutator,
  range?: TableCellRange | null,
): TableRangeEditResult | null {
  const base = resolveTableEdit(text, position);
  if (!base) return null;
  const cells = range ? clampRange(range, base) : singleCellRange(base.row, base.column);
  const ctx: TableEditContext = { ...base, row: cells.fromRow, column: cells.fromColumn };
  const lines = text.split("\n");
  const rows = lines.slice(ctx.startLine, ctx.endLine + 1);
  const target = mutate(rows, ctx, cells);
  if (!target) return null;
  const next = [...lines.slice(0, ctx.startLine), ...rows, ...lines.slice(ctx.endLine + 1)];

  const row = target.row <= 0 ? 0 : Math.max(1, target.row);
  const lineIndex = Math.min(ctx.startLine + inlineRowIndex(row), next.length - 1);
  const lineStart = next.slice(0, lineIndex).reduce((sum, item) => sum + item.length + 1, 0);
  return {
    text: next.join("\n"),
    cursor: lineStart + rowCellOffset(next[lineIndex] ?? "", target.column),
    range: target.range ?? singleCellRange(row, target.column),
  };
}

/** 设置选区内各列的对齐方式（只改分隔行）。 */
export function setTableAlign(
  text: string,
  position: number,
  align: TableAlign,
  range?: TableCellRange | null,
): TableRangeEditResult | null {
  return runTableEdit(text, position, (rows, ctx, cells) => {
    const index = ctx.delimiterLine - ctx.startLine;
    const line = splitRow(rows[index] ?? "");
    if (cells.fromColumn >= line.length) return null;
    const last = Math.min(cells.toColumn, line.length - 1);
    for (let column = cells.fromColumn; column <= last; column++) {
      line[column] = alignMarker(align, line[column].length);
    }
    rows[index] = joinRow(line);
    return { row: cells.fromRow, column: cells.fromColumn, range: cells };
  }, range);
}

/** 在选区上方 / 下方插入一空行（多行选区插在区域边界外）。
 *  表头行没有"上方"（新行若插到表头之上会顶掉表头）：统一插到第一个数据行之前。 */
export function insertTableRow(
  text: string,
  position: number,
  where: "above" | "below",
  range?: TableCellRange | null,
): TableRangeEditResult | null {
  return runTableEdit(text, position, (rows, ctx, cells) => {
    const blank = joinRow(Array.from({ length: ctx.columnCount }, () => ""));
    const insertAt = where === "above"
      ? cells.fromRow <= 0
        ? 2
        : inlineRowIndex(cells.fromRow)
      : cells.toRow <= 0
        ? 2
        : inlineRowIndex(cells.toRow) + 1;
    const focusRow = where === "above"
      ? cells.fromRow <= 0
        ? 1
        : cells.fromRow
      : cells.toRow + 1;
    rows.splice(insertAt, 0, blank);
    return { row: focusRow, column: cells.fromColumn };
  }, range);
}

/** 删除选区覆盖的数据行；表头行不可删除（只选中表头时不做任何改动）。 */
export function deleteTableRow(
  text: string,
  position: number,
  range?: TableCellRange | null,
): TableRangeEditResult | null {
  return runTableEdit(text, position, (rows, _ctx, cells) => {
    const from = Math.max(1, cells.fromRow); // 表头行始终保留
    if (cells.toRow < from) return null;
    rows.splice(inlineRowIndex(from), cells.toRow - from + 1);
    const dataRows = Math.max(0, rows.length - 2);
    return { row: dataRows === 0 ? 0 : Math.min(from, dataRows), column: cells.fromColumn };
  }, range);
}

/** 上移 / 下移选区覆盖的整块数据行；表头固定（表头行与第一数据行不可上移）、末行不可下移。 */
export function moveTableRow(
  text: string,
  position: number,
  direction: -1 | 1,
  range?: TableCellRange | null,
): TableRangeEditResult | null {
  return runTableEdit(text, position, (rows, _ctx, cells) => {
    const from = Math.max(1, cells.fromRow); // 表头行不参与移动
    if (cells.toRow < from) return null;
    const start = inlineRowIndex(from);
    const end = inlineRowIndex(cells.toRow);
    const length = end - start + 1;
    const target = direction < 0 ? start - 1 : end + 1;
    if (target < 2 || target >= rows.length) return null;
    const block = rows.splice(start, length);
    // 取出整块后其后的行会前移，插回位置按剩余行计算
    rows.splice(direction < 0 ? start - 1 : end + 2 - length, 0, ...block);
    return { row: from + direction, column: cells.fromColumn, range: { ...cells, fromRow: cells.fromRow + direction, toRow: cells.toRow + direction } };
  }, range);
}

/** 在选区左侧 / 右侧插入一列（分隔行新单元格为默认对齐）。 */
export function insertTableColumn(
  text: string,
  position: number,
  where: "left" | "right",
  range?: TableCellRange | null,
): TableRangeEditResult | null {
  return runTableEdit(text, position, (rows, _ctx, cells) => {
    const index = where === "left" ? cells.fromColumn : cells.toColumn + 1;
    rows.forEach((line, rowIndex) => {
      const lineCells = splitRow(line);
      if (cells.fromColumn >= lineCells.length) return;
      lineCells.splice(index, 0, rowIndex === 1 ? "---" : "");
      rows[rowIndex] = joinRow(lineCells);
    });
    return { row: cells.fromRow, column: index };
  }, range);
}

/** 删除选区覆盖的列；至少保留一列。 */
export function deleteTableColumn(
  text: string,
  position: number,
  range?: TableCellRange | null,
): TableRangeEditResult | null {
  return runTableEdit(text, position, (rows, ctx, cells) => {
    const count = cells.toColumn - cells.fromColumn + 1;
    if (ctx.columnCount - count < 1) return null; // 至少保留一列
    rows.forEach((line, rowIndex) => {
      const lineCells = splitRow(line);
      lineCells.splice(cells.fromColumn, count);
      rows[rowIndex] = joinRow(lineCells);
    });
    const column = Math.min(cells.fromColumn, ctx.columnCount - count - 1);
    return { row: cells.fromRow, column };
  }, range);
}

/** 左移 / 右移选区覆盖的整块列（含对齐）；首列不可左移、末列不可右移。 */
export function moveTableColumn(
  text: string,
  position: number,
  direction: -1 | 1,
  range?: TableCellRange | null,
): TableRangeEditResult | null {
  return runTableEdit(text, position, (rows, ctx, cells) => {
    const length = cells.toColumn - cells.fromColumn + 1;
    const target = direction < 0 ? cells.fromColumn - 1 : cells.toColumn + 1;
    if (target < 0 || target >= ctx.columnCount) return null;
    const insertAt = direction < 0 ? cells.fromColumn - 1 : cells.toColumn + 2 - length;
    rows.forEach((line, rowIndex) => {
      const lineCells = splitRow(line);
      const block = lineCells.splice(cells.fromColumn, length);
      lineCells.splice(insertAt, 0, ...block);
      rows[rowIndex] = joinRow(lineCells);
    });
    const column = direction < 0 ? cells.fromColumn - 1 : cells.fromColumn + 1;
    return {
      row: cells.fromRow,
      column,
      range: { ...cells, fromColumn: column, toColumn: cells.toColumn + direction },
    };
  }, range);
}

const FORMAT_MARKERS: Record<TableCellFormat, string> = {
  bold: "**",
  italic: "*",
  strike: "~~",
};

/** 值是否已带某格式的成对标记（斜体排除 `**` 加粗标记）。 */
function hasFormat(value: string, format: TableCellFormat): boolean {
  const marker = FORMAT_MARKERS[format];
  if (value.length < marker.length * 2 + 1) return false;
  if (!value.startsWith(marker) || !value.endsWith(marker)) return false;
  if (format === "italic" && value.startsWith("**")) return false;
  return true;
}

function stripFormat(value: string, format: TableCellFormat): string {
  const marker = FORMAT_MARKERS[format];
  return value.slice(marker.length, value.length - marker.length);
}

interface RangeCellValue {
  row: number;
  column: number;
  /** 该单元格所在表格行数组下标（含分隔行）。 */
  line: number;
  from: number;
  to: number;
  value: string;
}

/** 区域内单元格取值（跳过分隔行；越界列忽略）。 */
function rangeCellValues(
  rows: string[],
  ctx: TableEditContext,
  range: TableCellRange,
): RangeCellValue[] {
  const delimiter = ctx.delimiterLine - ctx.startLine;
  const values: RangeCellValue[] = [];
  for (let row = range.fromRow; row <= range.toRow; row++) {
    const line = inlineRowIndex(row);
    if (line === delimiter) continue; // 分隔行不参与格式化
    const text = rows[line] ?? "";
    const spans = rowValueSpans(text);
    for (let column = range.fromColumn; column <= range.toColumn; column++) {
      const span = spans[column];
      if (!span) continue;
      values.push({
        row,
        column,
        line,
        from: span.from,
        to: span.to,
        value: text.slice(span.from, span.to),
      });
    }
  }
  return values;
}

/** 区域内单元格加粗 / 斜体 / 删除线：区域内非空单元格都已带该格式 → 取消，否则补齐。
 *  只改目标单元格内容，其余字符（含空格与其它单元格）逐字保留。 */
export function formatTableCells(
  text: string,
  position: number,
  format: TableCellFormat,
  range?: TableCellRange | null,
): TableRangeEditResult | null {
  const marker = FORMAT_MARKERS[format];
  return runTableEdit(text, position, (rows, ctx, cells) => {
    const values = rangeCellValues(rows, ctx, cells).filter((item) => item.value !== "");
    if (!values.length) return null; // 区域内全为空：没有可格式化的内容
    const clearing = values.every((item) => hasFormat(item.value, format));
    // 同一行多格：从右往左替换，避免偏移互相影响
    for (const item of [...values].sort((a, b) => b.from - a.from)) {
      const value = clearing
        ? stripFormat(item.value, format)
        : hasFormat(item.value, format)
          ? item.value
          : `${marker}${item.value}${marker}`;
      const line = rows[item.line] ?? "";
      rows[item.line] = line.slice(0, item.from) + value + line.slice(item.to);
    }
    return { row: cells.fromRow, column: cells.fromColumn, range: cells };
  }, range);
}

/** 清空区域内单元格内容（多选后按 Delete / Backspace）。 */
export function clearTableCells(
  text: string,
  position: number,
  range?: TableCellRange | null,
): TableRangeEditResult | null {
  return runTableEdit(text, position, (rows, ctx, cells) => {
    const values = rangeCellValues(rows, ctx, cells).filter((item) => item.value !== "");
    if (!values.length) return null;
    // 同一行多格：从右往左替换，避免偏移互相影响
    for (const item of [...values].sort((a, b) => b.from - a.from)) {
      const line = rows[item.line] ?? "";
      rows[item.line] = line.slice(0, item.from) + line.slice(item.to);
    }
    return { row: cells.fromRow, column: cells.fromColumn, range: cells };
  }, range);
}

/** 表格工具窗的动作集合（与工具条按钮 data-act 一一对应；cells-clear 仅键盘触发）。 */
export type TableToolbarAction =
  | "align-left"
  | "align-center"
  | "align-right"
  | "align-default"
  | "bold"
  | "italic"
  | "strike"
  | "cells-clear"
  | "row-above"
  | "row-below"
  | "row-up"
  | "row-down"
  | "row-delete"
  | "col-insert-left"
  | "col-insert-right"
  | "col-move-left"
  | "col-move-right"
  | "col-delete";

/** 动作 → 表格编辑：UI 层只传动作名与选区，编辑规则集中在此。 */
export function applyTableRangeAction(
  text: string,
  position: number,
  range: TableCellRange | null,
  action: TableToolbarAction,
): TableRangeEditResult | null {
  switch (action) {
    case "align-left":
      return setTableAlign(text, position, "left", range);
    case "align-center":
      return setTableAlign(text, position, "center", range);
    case "align-right":
      return setTableAlign(text, position, "right", range);
    case "align-default":
      return setTableAlign(text, position, "default", range);
    case "bold":
      return formatTableCells(text, position, "bold", range);
    case "italic":
      return formatTableCells(text, position, "italic", range);
    case "strike":
      return formatTableCells(text, position, "strike", range);
    case "cells-clear":
      return clearTableCells(text, position, range);
    case "row-above":
      return insertTableRow(text, position, "above", range);
    case "row-below":
      return insertTableRow(text, position, "below", range);
    case "row-up":
      return moveTableRow(text, position, -1, range);
    case "row-down":
      return moveTableRow(text, position, 1, range);
    case "row-delete":
      return deleteTableRow(text, position, range);
    case "col-insert-left":
      return insertTableColumn(text, position, "left", range);
    case "col-insert-right":
      return insertTableColumn(text, position, "right", range);
    case "col-move-left":
      return moveTableColumn(text, position, -1, range);
    case "col-move-right":
      return moveTableColumn(text, position, 1, range);
    case "col-delete":
      return deleteTableColumn(text, position, range);
  }
  return null;
}

/** 工具条状态：一次解析出的表格上下文 + 选区 + 选区格式（UI 只读）。 */
export interface TableToolbarState {
  ctx: TableEditContext;
  /** 当前选区（无单元格选区时 = 光标所在单元格）。 */
  range: TableCellRange;
  /** 区域内非空单元格是否都已带该格式。 */
  formats: Record<TableCellFormat, boolean>;
  /** 选区内所有列对齐一致时给出该对齐，否则 null（对齐按钮 is-on 判据）。 */
  align: TableAlign | null;
  /** 区域内所有单元格都为空（格式化按钮无意义）。 */
  empty: boolean;
  /** 选区跨多行 / 多列。 */
  multi: boolean;
}

/** 解析工具条状态；position 不在表格内返回 null。 */
export function tableToolbarState(
  text: string,
  position: number,
  range?: TableCellRange | null,
): TableToolbarState | null {
  const base = resolveTableEdit(text, position);
  if (!base) return null;
  const cells = range ? clampRange(range, base) : singleCellRange(base.row, base.column);
  const ctx: TableEditContext = { ...base, row: cells.fromRow, column: cells.fromColumn };
  const rows = text.split("\n").slice(base.startLine, base.endLine + 1);
  const values = rangeCellValues(rows, ctx, cells).filter((item) => item.value !== "");
  const aligns = ctx.aligns.slice(cells.fromColumn, cells.toColumn + 1);
  return {
    ctx,
    range: cells,
    formats: {
      bold: values.length > 0 && values.every((item) => hasFormat(item.value, "bold")),
      italic: values.length > 0 && values.every((item) => hasFormat(item.value, "italic")),
      strike: values.length > 0 && values.every((item) => hasFormat(item.value, "strike")),
    },
    align: aligns.length > 0 && aligns.every((item) => item === aligns[0]) ? aligns[0] : null,
    empty: values.length === 0,
    multi: cells.fromRow !== cells.toRow || cells.fromColumn !== cells.toColumn,
  };
}

/** 按钮是否禁用（与编辑规则一致，避免点了没反应）。 */
export function tableActionDisabled(action: TableToolbarAction, state: TableToolbarState): boolean {
  const { ctx, range, empty } = state;
  switch (action) {
    case "bold":
    case "italic":
    case "strike":
      return empty;
    case "row-up":
      // 表头固定：Markdown 第一行必须是表头 → 表头行与第一数据行都不可上移
      return range.fromRow <= 1;
    case "row-down":
      return range.toRow >= ctx.rowCount - 1;
    case "row-delete":
      return range.toRow <= 0; // 只选中表头：没有可删除的数据行
    case "col-move-left":
      return range.fromColumn === 0;
    case "col-move-right":
      return range.toColumn >= ctx.columnCount - 1;
    case "col-delete":
      return ctx.columnCount - (range.toColumn - range.fromColumn + 1) < 1;
    default:
      return false;
  }
}

/** 单元格文本规范化：合并换行、转义竖线（否则会撑破表格结构）。 */
export function escapeCell(value: string): string {
  return value
    .replace(/[\r\n]+/g, " ")
    .replace(/(?<!\\)\|/g, "\\|")
    .trim();
}

/** 写回单元格内容：返回精确替换区间（渲染态直接编辑用，最小 diff）。 */
export function setTableCell(
  text: string,
  position: number,
  row: number,
  column: number,
  value: string,
): { from: number; to: number; insert: string } | null {
  const ctx = resolveTableEdit(text, position);
  if (!ctx) return null;
  if (row < 0 || row >= ctx.rowCount || column < 0 || column >= ctx.columnCount) return null;
  const lines = text.split("\n");
  const lineIndex = ctx.startLine + inlineRowIndex(row);
  const line = lines[lineIndex];
  if (line === undefined) return null;
  const bounds = cellBounds(line, column);
  if (!bounds) return null;
  const lineStart = lines.slice(0, lineIndex).reduce((sum, item) => sum + item.length + 1, 0);
  const escaped = escapeCell(value);
  return {
    from: lineStart + bounds.from,
    to: lineStart + bounds.to,
    // 清空单元格时保留两侧空格（`|  |`）：否则写完会变成 `||`，与其它行格式不一致，
    // 也会让「DOM 与源码是否一致」的判断把空单元格误判成有未提交输入
    insert: escaped ? ` ${escaped} ` : "  ",
  };
}

/** 调整列宽：只改分隔行该列（宽度 = 该列连字符基准，最少 3；保留对齐冒号）。 */
export function setColumnWidth(
  text: string,
  position: number,
  width: number,
  column?: number,
): TableRangeEditResult | null {
  return runTableEdit(text, position, (rows, ctx, cells) => {
    const index = ctx.delimiterLine - ctx.startLine;
    const line = splitRow(rows[index] ?? "");
    const target = column ?? cells.fromColumn;
    const cell = line[target];
    if (cell === undefined) return null;
    line[target] = alignMarker(parseAlignCell(cell), Math.max(3, Math.round(width)));
    rows[index] = joinRow(line);
    return { row: cells.fromRow, column: target, range: singleCellRange(cells.fromRow, target) };
  });
}

/** 各列显示宽度基准（分隔行单元格字符数，含对齐冒号）。 */
export function columnWidths(text: string, position: number): number[] {
  const ctx = resolveTableEdit(text, position);
  if (!ctx) return [];
  const lines = text.split("\n");
  return splitRow(lines[ctx.delimiterLine] ?? "").map((cell) => cell.length);
}
