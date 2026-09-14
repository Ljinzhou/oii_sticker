import type { SelectionRange, TransactionSpec } from "@codemirror/state";
import {
  handleEnterAtCursor,
  handleShiftTabAtCursor,
  handleTabAtCursor,
} from "../../../utils/edit-actions";
import { moveTableCell } from "./liveTables";
import {
  applyTableRangeAction,
  type TableCellRange,
  type TableToolbarAction,
} from "./liveTableEdits";

/** Build one minimal CodeMirror change for a pure text transformation. */
function toTransaction(
  before: string,
  after: string,
  cursor: number,
  userEvent: string,
): TransactionSpec | null {
  if (before === after) return null;
  let from = 0;
  while (from < before.length && from < after.length && before[from] === after[from]) from++;
  let beforeEnd = before.length;
  let afterEnd = after.length;
  while (beforeEnd > from && afterEnd > from && before[beforeEnd - 1] === after[afterEnd - 1]) {
    beforeEnd--;
    afterEnd--;
  }
  return {
    changes: { from, to: beforeEnd, insert: after.slice(from, afterEnd) },
    selection: { anchor: cursor },
    userEvent,
  };
}

function atLineStart(text: string, cursor: number): number {
  return text.lastIndexOf("\n", Math.max(0, cursor - 1)) + 1;
}

function inFenceAt(text: string, lineStart: number): boolean {
  let fence: "`" | "~" | null = null;
  for (const line of text.slice(0, lineStart).split("\n")) {
    const trimmed = line.trimStart();
    if (fence === null) {
      if (trimmed.startsWith("```")) fence = "`";
      else if (trimmed.startsWith("~~~")) fence = "~";
    } else if (
      (fence === "`" && trimmed.startsWith("```")) ||
      (fence === "~" && trimmed.startsWith("~~~"))
    ) {
      fence = null;
    }
  }
  return fence !== null;
}

function emptyListMarker(text: string, cursor: number): { lineStart: number; lineEnd: number; markerEnd: number } | null {
  const lineStart = atLineStart(text, cursor);
  if (inFenceAt(text, lineStart)) return null;
  const newline = text.indexOf("\n", cursor);
  const lineEnd = newline === -1 ? text.length : newline;
  const line = text.slice(lineStart, lineEnd);
  const leading = line.length - line.trimStart().length;
  const trimmed = line.slice(leading);
  const marker = /^(?:[-+*]\s+\[[ xX]\]\s+|[-+*]\s+|\d+(?:\.\d+)*[.)]\s+)/.exec(trimmed)?.[0];
  if (!marker || trimmed.slice(marker.length).trim() !== "") return null;
  const markerEnd = lineStart + leading + marker.length;
  if (cursor < markerEnd) return null;
  return { lineStart, lineEnd, markerEnd };
}

function transform(
  text: string,
  selection: SelectionRange,
  apply: (text: string, cursor: number) => { text: string; cursor: number } | null,
  userEvent: string,
): TransactionSpec | null {
  if (!selection.empty) return null;
  const result = apply(text, selection.head);
  return result ? toTransaction(text, result.text, result.cursor, userEvent) : null;
}

export function buildEnterTransaction(text: string, selection: SelectionRange): TransactionSpec | null {
  return transform(text, selection, (source, cursor) => handleEnterAtCursor(source, cursor), "input.complete");
}

export function buildTabTransaction(text: string, selection: SelectionRange): TransactionSpec | null {
  return transform(text, selection, (source, cursor) => handleTabAtCursor(source, cursor), "input.indent");
}

export function buildShiftTabTransaction(text: string, selection: SelectionRange): TransactionSpec | null {
  return transform(text, selection, (source, cursor) => handleShiftTabAtCursor(source, cursor), "input.dedent");
}

export function buildBackspaceTransaction(text: string, selection: SelectionRange): TransactionSpec | null {
  if (!selection.empty) return null;
  const marker = emptyListMarker(text, selection.head);
  if (!marker) return null;
  return toTransaction(text, text.slice(0, marker.lineStart) + text.slice(marker.lineEnd), marker.lineStart, "delete.backward");
}

function buildTableNavigation(text: string, selection: SelectionRange, direction: -1 | 1): TransactionSpec | null {
  if (!selection.empty) return null;
  const result = moveTableCell(text, selection.head, direction);
  if (!result) return null;
  return toTransaction(text, result.text, result.cursor, "input.table") ?? {
    selection: { anchor: result.cursor },
    userEvent: "input.table",
  };
}

export function buildTableForwardTransaction(text: string, selection: SelectionRange): TransactionSpec | null {
  return buildTableNavigation(text, selection, 1);
}

export function buildTableBackwardTransaction(text: string, selection: SelectionRange): TransactionSpec | null {
  return buildTableNavigation(text, selection, -1);
}

/** 表格工具窗动作 → 最小 CodeMirror 事务 + 编辑后应保持的选区。
 *  position 为表格内任意偏移（由工具条按表格 DOM 定位提供，而非编辑器光标）；
 *  range 为当前选中的单元格区域（多选时按区域批量操作）。 */
export function buildTableRangeTransaction(
  text: string,
  position: number,
  range: TableCellRange | null,
  action: TableToolbarAction,
): { spec: TransactionSpec; range: TableCellRange } | null {
  const result = applyTableRangeAction(text, position, range, action);
  if (!result) return null;
  const spec = toTransaction(text, result.text, result.cursor, "input.table");
  if (!spec) return null; // 无实际变化（例如对齐未变）时不打断撤销栈
  return { spec, range: result.range };
}

export function buildWrapTransaction(
  text: string,
  selection: SelectionRange,
  prefix: string,
  suffix: string,
  userEvent: string,
): TransactionSpec {
  const selected = text.slice(selection.from, selection.to);
  const insert = `${prefix}${selected}${suffix}`;
  const nextFrom = selection.from + prefix.length;
  return {
    changes: { from: selection.from, to: selection.to, insert },
    selection: selected
      ? { anchor: nextFrom, head: nextFrom + selected.length }
      : { anchor: selection.from + prefix.length },
    userEvent,
  };
}
