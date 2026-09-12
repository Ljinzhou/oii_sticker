import { describe, expect, it } from "vitest";
import {
  columnWidths,
  deleteTableColumn,
  deleteTableRow,
  insertTableColumn,
  insertTableRow,
  moveTableColumn,
  moveTableRow,
  setColumnWidth,
  setTableAlign,
  setTableCell,
  resolveTableEdit,
} from "./liveTableEdits";

// 三列：名称（默认左）/ 数量（右）/ 单价（居中）
const table = [
  "| 名称 | 数量 | 单价 |",
  "| --- | ---: | :---: |",
  "| 苹果 | 2 | 3.5 |",
  "| 香蕉 | 1 | 2.0 |",
].join("\n");

const lines = (text: string) => text.split("\n");
const lineOf = (text: string, pos: number) => text.slice(0, pos).split("\n").length - 1;
// 数据行（第 2 行）第 2 列「数量」单元格内的位置
const secondCol = table.indexOf("2 | 3.5");

describe("Markdown 表格结构化编辑（行）", () => {
  it("识别光标所在表格的行列与对齐", () => {
    const ctx = resolveTableEdit(table, table.indexOf("苹果"));
    expect(ctx).toMatchObject({ row: 1, column: 0, rowCount: 3, columnCount: 3 });
    expect(ctx?.aligns).toEqual(["default", "right", "center"]);
  });

  it("设置列对齐只改分隔行对应列，其余行原样保留", () => {
    const result = setTableAlign(table, table.indexOf("苹果"), "center");
    expect(lines(result!.text)).toEqual([
      "| 名称 | 数量 | 单价 |",
      "| :---: | ---: | :---: |",
      "| 苹果 | 2 | 3.5 |",
      "| 香蕉 | 1 | 2.0 |",
    ]);
  });

  it("清除对齐（默认）把分隔行该列还原为纯连字符", () => {
    const result = setTableAlign(table, table.indexOf("数量"), "default");
    expect(lines(result!.text)[1]).toBe("| --- | ---- | :---: |");
  });

  it("在上方 / 下方插入行：新行为等宽空单元格，光标落到新行同一列", () => {
    const above = insertTableRow(table, table.indexOf("苹果"), "above");
    expect(lines(above!.text)).toEqual([
      "| 名称 | 数量 | 单价 |",
      "| --- | ---: | :---: |",
      "|  |  |  |",
      "| 苹果 | 2 | 3.5 |",
      "| 香蕉 | 1 | 2.0 |",
    ]);
    expect(lineOf(above!.text, above!.cursor)).toBe(2);

    const below = insertTableRow(table, table.indexOf("苹果"), "below");
    expect(lines(below!.text)[3]).toBe("|  |  |  |");
    expect(lineOf(below!.text, below!.cursor)).toBe(3);
  });

  it("删除行：删掉光标所在行；表头行不可删除（返回 null）", () => {
    const removed = deleteTableRow(table, table.indexOf("苹果"));
    expect(lines(removed!.text)).toEqual([
      "| 名称 | 数量 | 单价 |",
      "| --- | ---: | :---: |",
      "| 香蕉 | 1 | 2.0 |",
    ]);
    expect(deleteTableRow(table, table.indexOf("名称"))).toBeNull();
  });

  it("上移 / 下移行：与相邻行交换；表头不可上移、末行不可下移", () => {
    const up = moveTableRow(table, table.indexOf("香蕉"), -1);
    expect(lines(up!.text).slice(2)).toEqual(["| 香蕉 | 1 | 2.0 |", "| 苹果 | 2 | 3.5 |"]);
    expect(lineOf(up!.text, up!.cursor)).toBe(2);

    const down = moveTableRow(table, table.indexOf("苹果"), 1);
    expect(lines(down!.text).slice(2)).toEqual(["| 香蕉 | 1 | 2.0 |", "| 苹果 | 2 | 3.5 |"]);
    expect(lineOf(down!.text, down!.cursor)).toBe(3);

    expect(moveTableRow(table, table.indexOf("名称"), -1)).toBeNull();
    expect(moveTableRow(table, table.indexOf("香蕉"), 1)).toBeNull();
  });
});

describe("Markdown 表格结构化编辑（列）", () => {
  it("在左侧 / 右侧插入列：所有行补齐单元格，分隔行用默认对齐", () => {
    const left = insertTableColumn(table, secondCol, "left");
    expect(lines(left!.text)).toEqual([
      "| 名称 |  | 数量 | 单价 |",
      "| --- | --- | ---: | :---: |",
      "| 苹果 |  | 2 | 3.5 |",
      "| 香蕉 |  | 1 | 2.0 |",
    ]);
    expect(lineOf(left!.text, left!.cursor)).toBe(2);

    const right = insertTableColumn(table, secondCol, "right");
    expect(lines(right!.text)[0]).toBe("| 名称 | 数量 |  | 单价 |");
    expect(lines(right!.text)[1]).toBe("| --- | ---: | --- | :---: |");
  });

  it("删除列：删掉光标所在列；只剩一列时不可删除", () => {
    const removed = deleteTableColumn(table, secondCol);
    expect(lines(removed!.text)).toEqual([
      "| 名称 | 单价 |",
      "| --- | :---: |",
      "| 苹果 | 3.5 |",
      "| 香蕉 | 2.0 |",
    ]);
    const single = "| 名称 |\n| --- |\n| 苹果 |";
    expect(deleteTableColumn(single, single.indexOf("苹果"))).toBeNull();
  });

  it("左移 / 右移列：整列交换（含对齐）；首列不可左移、末列不可右移", () => {
    const left = moveTableColumn(table, secondCol, -1);
    expect(lines(left!.text)).toEqual([
      "| 数量 | 名称 | 单价 |",
      "| ---: | --- | :---: |",
      "| 2 | 苹果 | 3.5 |",
      "| 1 | 香蕉 | 2.0 |",
    ]);
    expect(lineOf(left!.text, left!.cursor)).toBe(2);

    const right = moveTableColumn(table, secondCol, 1);
    expect(lines(right!.text)[0]).toBe("| 名称 | 单价 | 数量 |");
    expect(lines(right!.text)[1]).toBe("| --- | :---: | ---: |");

    expect(moveTableColumn(table, table.indexOf("名称"), -1)).toBeNull();
    expect(moveTableColumn(table, table.indexOf("单价"), 1)).toBeNull();
  });

describe("Markdown 表格结构化编辑（单元格与列宽）", () => {
  it("写回单元格内容：只替换该单元格，其余行原样保留", () => {
    const edit = setTableCell(table, table.indexOf("苹果"), 1, 1, "5");
    expect(edit).not.toBeNull();
    const next = table.slice(0, edit!.from) + edit!.insert + table.slice(edit!.to);
    expect(lines(next)).toEqual([
      "| 名称 | 数量 | 单价 |",
      "| --- | ---: | :---: |",
      "| 苹果 | 5 | 3.5 |",
      "| 香蕉 | 1 | 2.0 |",
    ]);
  });

  it("写回单元格内容：空单元格可写入，内容里的竖线自动转义", () => {
    const blank = "| 名称 | 数量 |\n| --- | --- |\n|  |  |";
    const edit = setTableCell(blank, blank.indexOf("名称"), 1, 0, "a|b");
    expect(edit).not.toBeNull();
    const next = blank.slice(0, edit!.from) + edit!.insert + blank.slice(edit!.to);
    expect(lines(next)[2]).toBe("| a\\|b |  |");
  });

  it("单元格越界（无此行/列）返回 null", () => {
    expect(setTableCell(table, table.indexOf("苹果"), 9, 0, "x")).toBeNull();
    expect(setTableCell(table, table.indexOf("苹果"), 1, 9, "x")).toBeNull();
  });

  it("调整列宽只改分隔行该列（连字符数 = 目标宽度，最少 3）", () => {
    const wider = setColumnWidth(table, table.indexOf("苹果"), 8);
    expect(lines(wider!.text)[1]).toBe("| -------- | ---: | :---: |");

    const narrower = setColumnWidth(table, table.indexOf("苹果"), 1);
    expect(lines(narrower!.text)[1]).toBe("| --- | ---: | :---: |");
    // 内容行不受影响
    expect(lines(narrower!.text)[2]).toBe("| 苹果 | 2 | 3.5 |");
  });

  it("列宽解析：带冒号的对齐标记不改变列宽基准", () => {
    expect(columnWidths(table, table.indexOf("苹果"))).toEqual([3, 4, 5]);
    const blank = "| 名称 | 数量 |\n| --- | --- |\n|  |  |";
    expect(columnWidths(blank, blank.indexOf("名称"))).toEqual([3, 3]);
  });
});

  it("表格外的光标不做任何编辑", () => {
    const text = `${table}\n\n正文`;
    expect(setTableAlign(text, text.indexOf("正文"), "center")).toBeNull();
    expect(insertTableRow(text, text.indexOf("正文"), "below")).toBeNull();
  });
});
