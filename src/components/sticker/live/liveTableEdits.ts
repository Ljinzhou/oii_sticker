// Markdown 表格的结构化编辑内核（表格工具窗的操作层）。
// 设计要点：
// - 只改必要的行，其余行保持原样（整表重排会产生大 diff 并抹掉用户格式）；
// - 全部为纯函数：输入原文 + 光标位置，输出新文本 + 新光标位置，
//   由 liveTransforms 包成最小 CodeMirror Transaction；
// - 所有写操作都保证表格结构合法（表头 + 分隔行始终存在、分隔行始终在第二行）。
import { parseTableAt } from "./liveTables";

/** 列对齐；default = 无冒号（Markdown 默认左对齐）。 */
export type TableAlign = "default" | "left" | "center" | "right";

export interface TableEditContext {
  /** 表格首行（表头行）的文档行号。 */
  startLine: number;
  /** 表格末行的文档行号。 */
  endLine: number;
  /** 分隔行的文档行号。 */
  delimiterLine: number;
  /** 光标所在行（0 = 表头行）。 */
  row: number;
  /** 光标所在列。 */
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

/** 生成分隔行单元格；宽度尽量沿用原宽度，连字符最少 3 个（GFM 要求）。 */
function alignCell(align: TableAlign, width: number): string {
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

/** 表格内"视觉行" → 表格行数组下标（含分隔行）。 */
function inlineRowIndex(row: number): number {
  return row === 0 ? 0 : row + 1;
}

type TableMutator = (rows: string[], ctx: TableEditContext) => { row: number; column: number } | null;

/** 统一入口：在表格行数组上执行改动，再算回绝对光标位置。 */
function runTableEdit(text: string, position: number, mutate: TableMutator): TableEditResult | null {
  const ctx = resolveTableEdit(text, position);
  if (!ctx) return null;
  const lines = text.split("\n");
  const rows = lines.slice(ctx.startLine, ctx.endLine + 1);
  const target = mutate(rows, ctx);
  if (!target) return null;
  const next = [...lines.slice(0, ctx.startLine), ...rows, ...lines.slice(ctx.endLine + 1)];

  const row = target.row === 0 ? 0 : Math.max(1, target.row);
  const lineIndex = Math.min(ctx.startLine + inlineRowIndex(row), next.length - 1);
  const lineStart = next.slice(0, lineIndex).reduce((sum, item) => sum + item.length + 1, 0);
  return {
    text: next.join("\n"),
    cursor: lineStart + rowCellOffset(next[lineIndex] ?? "", target.column),
  };
}

/** 设置光标所在列的对齐方式（只改分隔行）。 */
export function setTableAlign(text: string, position: number, align: TableAlign): TableEditResult | null {
  return runTableEdit(text, position, (rows, ctx) => {
    const index = ctx.delimiterLine - ctx.startLine;
    const cells = splitRow(rows[index] ?? "");
    if (ctx.column >= cells.length) return null;
    cells[ctx.column] = alignCell(align, cells[ctx.column].length);
    rows[index] = joinRow(cells);
    return { row: ctx.row, column: ctx.column };
  });
}

/** 在光标所在行的上方 / 下方插入一空行。
 *  表头行没有"上方"（新行若插到表头之上会顶掉表头）：统一插到第一个数据行之前。 */
export function insertTableRow(
  text: string,
  position: number,
  where: "above" | "below",
): TableEditResult | null {
  return runTableEdit(text, position, (rows, ctx) => {
    const blank = joinRow(Array.from({ length: ctx.columnCount }, () => ""));
    if (ctx.row === 0) {
      rows.splice(2, 0, blank);
      return { row: 1, column: ctx.column };
    }
    const index = inlineRowIndex(ctx.row);
    rows.splice(where === "above" ? index : index + 1, 0, blank);
    return { row: where === "above" ? ctx.row : ctx.row + 1, column: ctx.column };
  });
}

/** 删除光标所在行；表头行不可删除。 */
export function deleteTableRow(text: string, position: number): TableEditResult | null {
  return runTableEdit(text, position, (rows, ctx) => {
    if (ctx.row === 0) return null;
    rows.splice(inlineRowIndex(ctx.row), 1);
    const dataRows = Math.max(0, rows.length - 2);
    return { row: dataRows === 0 ? 0 : Math.min(ctx.row, dataRows), column: ctx.column };
  });
}

/** 上移 / 下移光标所在行；表头行不可上移，末行不可下移。 */
export function moveTableRow(text: string, position: number, direction: -1 | 1): TableEditResult | null {
  return runTableEdit(text, position, (rows, ctx) => {
    if (ctx.row === 0) return null;
    const index = inlineRowIndex(ctx.row);
    const target = index + direction;
    if (target < 2 || target >= rows.length) return null;
    [rows[index], rows[target]] = [rows[target], rows[index]];
    return { row: ctx.row + direction, column: ctx.column };
  });
}

/** 在光标所在列的左侧 / 右侧插入一列（分隔行新单元格为默认对齐）。 */
export function insertTableColumn(
  text: string,
  position: number,
  where: "left" | "right",
): TableEditResult | null {
  return runTableEdit(text, position, (rows, ctx) => {
    const index = where === "left" ? ctx.column : ctx.column + 1;
    rows.forEach((line, rowIndex) => {
      const cells = splitRow(line);
      if (ctx.column >= cells.length) return;
      cells.splice(index, 0, rowIndex === 1 ? "---" : "");
      rows[rowIndex] = joinRow(cells);
    });
    return { row: ctx.row, column: index };
  });
}

/** 删除光标所在列；只剩一列时不可删除。 */
export function deleteTableColumn(text: string, position: number): TableEditResult | null {
  return runTableEdit(text, position, (rows, ctx) => {
    if (ctx.columnCount <= 1) return null;
    rows.forEach((line, rowIndex) => {
      const cells = splitRow(line);
      cells.splice(ctx.column, 1);
      rows[rowIndex] = joinRow(cells);
    });
    return { row: ctx.row, column: Math.max(0, Math.min(ctx.column, ctx.columnCount - 2)) };
  });
}

/** 左移 / 右移光标所在列（整列交换，含对齐）；首列不可左移，末列不可右移。 */
export function moveTableColumn(text: string, position: number, direction: -1 | 1): TableEditResult | null {
  return runTableEdit(text, position, (rows, ctx) => {
    const target = ctx.column + direction;
    if (target < 0 || target >= ctx.columnCount) return null;
    rows.forEach((line, rowIndex) => {
      const cells = splitRow(line);
      [cells[ctx.column], cells[target]] = [cells[target], cells[ctx.column]];
      rows[rowIndex] = joinRow(cells);
    });
    return { row: ctx.row, column: target };
  });
}

/** 表格工具窗的动作集合（与工具条按钮 data-act 一一对应）。 */
export type TableToolbarAction =
  | "align-left"
  | "align-center"
  | "align-right"
  | "align-default"
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

/** 动作 → 表格编辑：UI 层只传动作名，编辑规则集中在此。 */
export function applyTableAction(
  text: string,
  position: number,
  action: TableToolbarAction,
): TableEditResult | null {
  switch (action) {
    case "align-left":
      return setTableAlign(text, position, "left");
    case "align-center":
      return setTableAlign(text, position, "center");
    case "align-right":
      return setTableAlign(text, position, "right");
    case "align-default":
      return setTableAlign(text, position, "default");
    case "row-above":
      return insertTableRow(text, position, "above");
    case "row-below":
      return insertTableRow(text, position, "below");
    case "row-up":
      return moveTableRow(text, position, -1);
    case "row-down":
      return moveTableRow(text, position, 1);
    case "row-delete":
      return deleteTableRow(text, position);
    case "col-insert-left":
      return insertTableColumn(text, position, "left");
    case "col-insert-right":
      return insertTableColumn(text, position, "right");
    case "col-move-left":
      return moveTableColumn(text, position, -1);
    case "col-move-right":
      return moveTableColumn(text, position, 1);
    case "col-delete":
      return deleteTableColumn(text, position);
  }
  return null;
}

/** 按钮是否禁用（与编辑规则一致，避免点了没反应）。 */
export function tableActionDisabled(action: TableToolbarAction, ctx: TableEditContext): boolean {
  switch (action) {
    case "row-up":
    case "row-delete":
      return ctx.row === 0;
    case "row-down":
      return ctx.row === 0 || ctx.row >= ctx.rowCount - 1;
    case "col-move-left":
      return ctx.column === 0;
    case "col-move-right":
      return ctx.column >= ctx.columnCount - 1;
    case "col-delete":
      return ctx.columnCount <= 1;
    default:
      return false;
  }
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
    insert: escaped ? ` ${escaped} ` : "",
  };
}

/** 调整列宽：只改分隔行该列（宽度 = 该列连字符基准，最少 3；保留对齐冒号）。 */
export function setColumnWidth(text: string, position: number, width: number): TableEditResult | null {
  return runTableEdit(text, position, (rows, ctx) => {
    const index = ctx.delimiterLine - ctx.startLine;
    const cells = splitRow(rows[index] ?? "");
    const cell = cells[ctx.column];
    if (cell === undefined) return null;
    cells[ctx.column] = alignCell(parseAlignCell(cell), Math.max(3, Math.round(width)));
    rows[index] = joinRow(cells);
    return { row: ctx.row, column: ctx.column };
  });
}

/** 各列显示宽度基准（分隔行单元格字符数，含对齐冒号）。 */
export function columnWidths(text: string, position: number): number[] {
  const ctx = resolveTableEdit(text, position);
  if (!ctx) return [];
  const lines = text.split("\n");
  return splitRow(lines[ctx.delimiterLine] ?? "").map((cell) => cell.length);
}
