import { describe, expect, it } from "vitest";
import { parseTableAt, moveTableCell } from "./liveTables";

const table = "| 名称 | 数量 |\n| --- | ---: |\n| 苹果 | 2 |";

describe("Live Preview Markdown table navigation", () => {
  it("识别表头、分隔行和数据单元格源范围", () => {
    const model = parseTableAt(table, table.indexOf("苹果"));
    expect(model).not.toBeNull();
    expect(model?.delimiterLine).toBe(1);
    expect(model?.cells.map((cell) => table.slice(cell.from, cell.to))).toEqual([
      "名称",
      "数量",
      "苹果",
      "2",
    ]);
  });

  it("Tab 在单元格间移动，末单元格新增一行", () => {
    const first = table.indexOf("名称");
    const second = table.indexOf("数量");
    const third = table.indexOf("苹果");
    const fourth = table.indexOf("2");
    expect(moveTableCell(table, first, 1)?.cursor).toBe(second);
    expect(moveTableCell(table, fourth, 1)).toMatchObject({
      text: `${table}\n|  |  |`,
      cursor: table.length + 2,
    });
    expect(moveTableCell(table, third, -1)?.cursor).toBe(second);
  });
});
