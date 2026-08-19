export interface TableCell {
  from: number;
  to: number;
  row: number;
  column: number;
}

export interface TableModel {
  startLine: number;
  endLine: number;
  delimiterLine: number;
  cells: TableCell[];
}

function lineStarts(text: string): number[] {
  const starts: number[] = [];
  let offset = 0;
  for (const line of text.split("\n")) {
    starts.push(offset);
    offset += line.length + 1;
  }
  return starts;
}

function isTableRow(line: string): boolean {
  return /^\s*\|.*\|\s*$/.test(line);
}

function isDelimiterRow(line: string): boolean {
  if (!isTableRow(line)) return false;
  const cells = line.trim().replace(/^\|/, "").replace(/\|$/, "").split("|");
  return cells.length >= 1 && cells.every((cell) => /^\s*:?-{3,}:?\s*$/.test(cell));
}

function parseCells(line: string, start: number, row: number): TableCell[] {
  const cells: TableCell[] = [];
  const pipes: number[] = [];
  for (let index = 0; index < line.length; index++) {
    if (line[index] === "|") pipes.push(index);
  }
  const first = line.trimStart().startsWith("|") ? 0 : -1;
  const last = line.trimEnd().endsWith("|") ? pipes.length - 1 : pipes.length;
  for (let index = first; index < last; index++) {
    const segmentStart = index < 0 ? 0 : pipes[index] + 1;
    const segmentEnd = index + 1 < pipes.length ? pipes[index + 1] : line.length;
    const raw = line.slice(segmentStart, segmentEnd);
    const leading = raw.length - raw.trimStart().length;
    const trailing = raw.trimEnd().length;
    cells.push({
      from: start + segmentStart + leading,
      to: start + segmentStart + trailing,
      row,
      column: index - first,
    });
  }
  return cells;
}

export function parseTableAt(text: string, position: number): TableModel | null {
  const lines = text.split("\n");
  const starts = lineStarts(text);
  const positionLine = lines.findIndex((line, index) => {
    const from = starts[index];
    return position >= from && position <= from + line.length;
  });
  for (let header = 0; header < lines.length - 1; header++) {
    if (!isTableRow(lines[header]) || !isDelimiterRow(lines[header + 1])) continue;
    let end = header + 1;
    while (end + 1 < lines.length && isTableRow(lines[end + 1])) end++;
    if (positionLine < header || positionLine > end) continue;
    const cells = [
      ...parseCells(lines[header], starts[header], 0),
      ...lines.slice(header + 2, end + 1).flatMap((line, index) =>
        parseCells(line, starts[header + 2 + index], index + 1),
      ),
    ];
    return { startLine: header, endLine: end, delimiterLine: header + 1, cells };
  }
  return null;
}

export function moveTableCell(
  text: string,
  position: number,
  direction: -1 | 1,
): { text: string; cursor: number } | null {
  const table = parseTableAt(text, position);
  if (!table) return null;
  const currentIndex = table.cells.findIndex((cell) => position >= cell.from && position <= cell.to);
  if (currentIndex < 0) return null;
  const nextIndex = currentIndex + direction;
  if (nextIndex >= 0 && nextIndex < table.cells.length) {
    return { text, cursor: table.cells[nextIndex].from };
  }
  if (direction < 0) return null;
  const columns = table.cells.filter((cell) => cell.row === 0).length;
  const lines = text.split("\n");
  const end = lineStarts(text)[table.endLine] + lines[table.endLine].length;
  const row = `| ${Array.from({ length: columns }, () => "").join(" | ")} |`;
  const inserted = `${text.slice(0, end)}\n${row}${text.slice(end)}`;
  return { text: inserted, cursor: end + 2 };
}
