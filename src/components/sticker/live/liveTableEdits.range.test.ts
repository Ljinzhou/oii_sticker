import { describe, expect, it } from "vitest";
import {
  clearTableCells,
  formatTableCells,
  insertTableColumn,
  insertTableRow,
  moveTableColumn,
  moveTableRow,
  setTableAlign,
  tableActionDisabled,
  tableToolbarState,
  type TableCellRange,
} from "./liveTableEdits";

// 三列：名称（默认左）/ 数量（右）/ 单价（居中）
const table = [
  "| 名称 | 数量 | 单价 |",
  "| --- | ---: | :---: |",
  "| 苹果 | 2 | 3.5 |",
  "| 香蕉 | 1 | 2.0 |",
].join("\n");

const lines = (text: string) => text.split("\n");
/** 单格选区（行列都相同）。 */
const cell = (row: number, column: number): TableCellRange => ({
  fromRow: row,
  toRow: row,
  fromColumn: column,
  toColumn: column,
});
/** 表格定位点：用表头单元格的偏移解析这张表（行列由选区显式给出）。 */
const at = table.indexOf("名称");

describe("表格区域编辑：插入落在选中的单元格处（而非固定第一行/第一列）", () => {
  it("上方插入行落在选中行之上，下方插入落在选中行之下", () => {
    const above = insertTableRow(table, at, "above", cell(2, 1));
    expect(lines(above!.text)).toEqual([
      "| 名称 | 数量 | 单价 |",
      "| --- | ---: | :---: |",
      "| 苹果 | 2 | 3.5 |",
      "|  |  |  |",
      "| 香蕉 | 1 | 2.0 |",
    ]);

    const below = insertTableRow(table, at, "below", cell(1, 2));
    expect(lines(below!.text)).toEqual([
      "| 名称 | 数量 | 单价 |",
      "| --- | ---: | :---: |",
      "| 苹果 | 2 | 3.5 |",
      "|  |  |  |",
      "| 香蕉 | 1 | 2.0 |",
    ]);
  });

  it("多行选区：在上方 / 下方插入到选区外沿，并保持选中新行", () => {
    const below = insertTableRow(table, at, "below", { fromRow: 1, toRow: 2, fromColumn: 0, toColumn: 1 });
    expect(lines(below!.text)[4]).toBe("|  |  |  |");
    expect(below!.range).toEqual(cell(3, 0));

    const above = insertTableRow(table, at, "above", { fromRow: 1, toRow: 2, fromColumn: 0, toColumn: 1 });
    expect(lines(above!.text)[2]).toBe("|  |  |  |");
    expect(above!.range).toEqual(cell(1, 0));
  });

  it("表头行上方插入 → 视为第一个数据行之前（表头始终在首行）", () => {
    const result = insertTableRow(table, at, "above", cell(0, 0));
    expect(lines(result!.text)).toEqual([
      "| 名称 | 数量 | 单价 |",
      "| --- | ---: | :---: |",
      "|  |  |  |",
      "| 苹果 | 2 | 3.5 |",
      "| 香蕉 | 1 | 2.0 |",
    ]);
  });

  it("左侧 / 右侧插入列落在选中列两侧", () => {
    const left = insertTableColumn(table, at, "left", cell(1, 2));
    expect(lines(left!.text)).toEqual([
      "| 名称 | 数量 |  | 单价 |",
      "| --- | ---: | --- | :---: |",
      "| 苹果 | 2 |  | 3.5 |",
      "| 香蕉 | 1 |  | 2.0 |",
    ]);
    expect(left!.range).toEqual(cell(1, 2));

    const right = insertTableColumn(table, at, "right", cell(1, 1));
    expect(lines(right!.text)).toEqual([
      "| 名称 | 数量 |  | 单价 |",
      "| --- | ---: | --- | :---: |",
      "| 苹果 | 2 |  | 3.5 |",
      "| 香蕉 | 1 |  | 2.0 |",
    ]);
  });
});

describe("表格区域编辑：选区内的批量操作", () => {
  it("对齐：作用于选区覆盖的所有列（其余列不动）", () => {
    const result = setTableAlign(table, at, "center", { fromRow: 1, toRow: 2, fromColumn: 0, toColumn: 1 });
    expect(lines(result!.text)).toEqual([
      "| 名称 | 数量 | 单价 |",
      "| :---: | :---: | :---: |",
      "| 苹果 | 2 | 3.5 |",
      "| 香蕉 | 1 | 2.0 |",
    ]);
    expect(result!.range).toEqual({ fromRow: 1, toRow: 2, fromColumn: 0, toColumn: 1 });
  });

  it("加粗 / 斜体 / 删除线：区域内单元格整体加标记，再按一次整体取消", () => {
    const bold = formatTableCells(table, at, "bold", cell(1, 0));
    expect(lines(bold!.text)[2]).toBe("| **苹果** | 2 | 3.5 |");
    const unbold = formatTableCells(bold!.text, bold!.text.indexOf("名称"), "bold", cell(1, 0));
    expect(lines(unbold!.text)[2]).toBe("| 苹果 | 2 | 3.5 |");

    const strike = formatTableCells(table, at, "strike", { fromRow: 1, toRow: 2, fromColumn: 0, toColumn: 1 });
    expect(lines(strike!.text).slice(2)).toEqual([
      "| ~~苹果~~ | ~~2~~ | 3.5 |",
      "| ~~香蕉~~ | ~~1~~ | 2.0 |",
    ]);

    // 斜体不与加粗混淆：`**x**` 不视为已斜体
    const italic = formatTableCells(bold!.text, bold!.text.indexOf("名称"), "italic", cell(1, 0));
    expect(lines(italic!.text)[2]).toBe("| ***苹果*** | 2 | 3.5 |");
  });

  it("空选区不做格式化（返回 null，避免插入空标记）", () => {
    const blank = "| 名称 | 数量 |\n| --- | --- |\n|  |  |";
    expect(formatTableCells(blank, blank.indexOf("名称"), "bold", cell(1, 0))).toBeNull();
  });

  it("清空选区内容：只清选中的单元格", () => {
    const result = clearTableCells(table, at, { fromRow: 1, toRow: 2, fromColumn: 0, toColumn: 1 });
    expect(lines(result!.text).slice(2)).toEqual(["|  |  | 3.5 |", "|  |  | 2.0 |"]);
  });
});

describe("表格区域编辑：整块移动与删改", () => {
  const wide = [
    "| A | B |",
    "| --- | --- |",
    "| 1 | a |",
    "| 2 | b |",
    "| 3 | c |",
  ].join("\n");
  const wideAt = wide.indexOf("A");

  it("整块下移 / 上移：选中的多行保持相对顺序", () => {
    const down = moveTableRow(wide, wideAt, 1, { fromRow: 1, toRow: 2, fromColumn: 0, toColumn: 1 });
    expect(lines(down!.text).slice(2)).toEqual(["| 3 | c |", "| 1 | a |", "| 2 | b |"]);
    expect(down!.range).toEqual({ fromRow: 2, toRow: 3, fromColumn: 0, toColumn: 1 });

    const up = moveTableRow(wide, wideAt, -1, { fromRow: 3, toRow: 3, fromColumn: 0, toColumn: 0 });
    expect(lines(up!.text).slice(2)).toEqual(["| 1 | a |", "| 3 | c |", "| 2 | b |"]);
  });

  it("整块右移列：含对齐一起移动", () => {
    const right = moveTableColumn(wide, wideAt, 1, { fromRow: 0, toRow: 1, fromColumn: 0, toColumn: 0 });
    expect(lines(right!.text)).toEqual([
      "| B | A |",
      "| --- | --- |",
      "| a | 1 |",
      "| b | 2 |",
      "| c | 3 |",
    ]);
  });

  it("界面：工具条按钮按选中单元格给出可用性（表头固定 + 边界）", () => {
    const body = tableToolbarState(table, at, cell(2, 1))!;
    expect(body.ctx.row).toBe(2);
    expect(body.ctx.column).toBe(1);
    expect(tableActionDisabled("row-up", body)).toBe(false); // 末数据行可上移
    expect(tableActionDisabled("row-down", body)).toBe(true); // 末行不可下移
    expect(tableActionDisabled("col-move-left", body)).toBe(false);
    expect(tableActionDisabled("col-move-right", body)).toBe(false); // 第 2 列（共 3 列）可右移
    expect(tableActionDisabled("row-delete", body)).toBe(false);

    // 表头行与第一数据行不可上移（Markdown 第一行必须是表头）
    const header = tableToolbarState(table, at, cell(0, 0))!;
    expect(tableActionDisabled("row-up", header)).toBe(true);
    expect(tableActionDisabled("row-delete", header)).toBe(true);
    expect(tableActionDisabled("col-move-left", header)).toBe(true);
    const firstData = tableToolbarState(table, at, cell(1, 0))!;
    expect(tableActionDisabled("row-up", firstData)).toBe(true);

    // 末行 / 末列不可继续移动
    const last = tableToolbarState(table, at, cell(2, 2))!;
    expect(tableActionDisabled("row-down", last)).toBe(true);
    expect(tableActionDisabled("col-move-right", last)).toBe(true);
  });

  it("界面：格式按钮的 is-on 依据选区内容（空白单元格禁用）", () => {
    const bolded = formatTableCells(table, at, "bold", { fromRow: 1, toRow: 2, fromColumn: 0, toColumn: 0 })!;
    const state = tableToolbarState(bolded.text, bolded.text.indexOf("名称"), {
      fromRow: 1,
      toRow: 2,
      fromColumn: 0,
      toColumn: 0,
    })!;
    expect(state.formats.bold).toBe(true);
    expect(state.formats.italic).toBe(false);
    expect(state.align).toBe("default");
    expect(state.multi).toBe(true);
    expect(tableActionDisabled("bold", state)).toBe(false);

    const blank = "| 名称 | 数量 |\n| --- | --- |\n|  |  |";
    const emptyState = tableToolbarState(blank, blank.indexOf("名称"), cell(1, 0))!;
    expect(emptyState.empty).toBe(true);
    expect(tableActionDisabled("bold", emptyState)).toBe(true);
  });
});
