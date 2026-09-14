import { describe, expect, it } from "vitest";
import { parseTableAt } from "./liveTables";
import {
  columnWidths,
  insertTableColumn,
  insertTableRow,
  tableToolbarState,
} from "./liveTableEdits";

// GFM 允许的手写表格写法：判定必须与 markdown-it / lezer 一致。
// 判定更严格会出现「渲染成表格却解析不到表格」——表现为选中单元格不弹工具条。
const forms: Array<[string, string, string[]]> = [
  ["标准 3 连字符", "| 维度 | 分值 |\n| --- | --- |\n| 技能水平 | 60 |", ["维度", "分值", "技能水平", "60"]],
  ["分隔行 1 个连字符", "| 观察点 | 分值 |\n| - | - |\n| 操作规范性 | 10 |", ["观察点", "分值", "操作规范性", "10"]],
  ["分隔行 2 连字符 + 对齐冒号", "| 维度 | 分值 |\n| -- | :--: |\n| 应用价值 | 10 |", ["维度", "分值", "应用价值", "10"]],
  ["省略首尾竖线", "维度 | 分值\n--- | ---\n技能水平 | 60", ["维度", "分值", "技能水平", "60"]],
];

describe("GFM 表格写法兼容（判定与渲染器一致）", () => {
  for (const [name, text, values] of forms) {
    it(`${name}：解析出表格、分隔行与单元格`, () => {
      const model = parseTableAt(text, 1); // 表头行内
      expect(model).not.toBeNull();
      expect(model!.delimiterLine).toBe(1);
      expect(model!.cells.map((cell) => text.slice(cell.from, cell.to))).toEqual(values);
    });

    it(`${name}：工具条状态可用（选中单元格会弹工具条）`, () => {
      const state = tableToolbarState(text, 1, null);
      expect(state).not.toBeNull();
      expect(state!.ctx.rowCount).toBe(2);
      expect(state!.ctx.columnCount).toBe(2);
      expect(state!.empty).toBe(false);
    });
  }

  it("手写写法（| - |）也能执行插入行列 / 移动", () => {
    const text = "| 观察点 | 分值 |\n| - | - |\n| 操作规范性 | 10 |";
    const column = insertTableColumn(text, 1, "right", {
      fromRow: 1,
      toRow: 1,
      fromColumn: 0,
      toColumn: 0,
    });
    expect(column!.text.split("\n")[0]).toBe("| 观察点 |  | 分值 |");

    const row = insertTableRow(text, 1, "below", {
      fromRow: 1,
      toRow: 1,
      fromColumn: 1,
      toColumn: 1,
    });
    expect(row!.text.split("\n")[3]).toBe("|  |  |");
  });

  it("分隔行格数与表头不一致：列宽按表头列数补齐（列宽不再错位）", () => {
    const text = "| 观察点 | 分值 | 官方说明 |\n| --- | --- |\n| 操作规范性 | 10 | 说明 |";
    expect(columnWidths(text, 1)).toHaveLength(3);
    // 多余的分隔行格数按表头裁剪
    const extra = "| 观察点 |\n| --- | --- | --- |\n| 操作规范性 |";
    expect(columnWidths(extra, 1)).toHaveLength(1);
  });

  it("单元格内的转义竖线 \\| 不切断单元格", () => {
    const text = "| 说明 | 值 |\n| --- | --- |\n| a\\|b | 1 |";
    const model = parseTableAt(text, 1);
    expect(model!.cells.map((cell) => text.slice(cell.from, cell.to))).toEqual([
      "说明",
      "值",
      "a\\|b",
      "1",
    ]);
  });
});
