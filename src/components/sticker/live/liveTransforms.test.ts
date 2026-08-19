import { describe, expect, it } from "vitest";
import { EditorSelection } from "@codemirror/state";
import {
  buildEnterTransaction,
  buildTabTransaction,
  buildShiftTabTransaction,
  buildBackspaceTransaction,
  buildTableForwardTransaction,
  buildTableBackwardTransaction,
  buildWrapTransaction,
} from "./liveTransforms";

describe("Live Preview Markdown transaction builders", () => {
  it("回车在有序列表中生成递增的下一项", () => {
    const text = "1. 第一项";
    const spec = buildEnterTransaction(text, EditorSelection.cursor(text.length));
    expect(spec).toMatchObject({
      changes: { from: text.length, to: text.length, insert: "\n2. " },
      selection: { anchor: text.length + 4 },
      userEvent: "input.complete",
    });
  });

  it("Tab 和 Shift+Tab 使用单个可撤销事务", () => {
    const text = "1. 父项\n2. 子项";
    const tab = buildTabTransaction(text, EditorSelection.cursor(text.length));
    expect(tab?.changes).toEqual({ from: 6, to: 8, insert: "  1.1" });
    expect(tab?.selection).toMatchObject({ anchor: text.length + 3 });

    const indented = "1. 父项\n  1.1 子项";
    const shiftTab = buildShiftTabTransaction(
      indented,
      EditorSelection.cursor(indented.length),
    );
    expect(shiftTab?.changes).toEqual({ from: 6, to: 11, insert: "2." });
    expect(shiftTab?.userEvent).toBe("input.dedent");
  });

  it("空列表项退格退出列表并保留换行", () => {
    const text = "- 文本\n- ";
    const spec = buildBackspaceTransaction(text, EditorSelection.cursor(text.length));
    expect(spec).toMatchObject({
      changes: { from: 5, to: text.length, insert: "" },
      selection: { anchor: 5 },
      userEvent: "delete.backward",
    });
  });

  it("表格 Tab 导航和末单元格新增行使用单事务", () => {
    const text = "| A | B |\n| --- | --- |\n| 1 | 2 |";
    const last = text.indexOf("2");
    const next = buildTableForwardTransaction(text, EditorSelection.cursor(last));
    expect(next?.changes).toMatchObject({ insert: "\n|  |  |" });
    expect(next?.userEvent).toBe("input.table");
    const back = buildTableBackwardTransaction(text, EditorSelection.cursor(last));
    expect(back).toMatchObject({ selection: { anchor: text.indexOf("1") } });
  });

  it("快捷键包装选区或在光标处插入成对标记", () => {
    const selected = "正文";
    const selectedSpec = buildWrapTransaction(selected, EditorSelection.range(0, 2), "**", "**", "input.format.bold");
    expect(selectedSpec).toMatchObject({ changes: { insert: "**正文**" }, selection: { anchor: 2, head: 4 } });
    const emptySpec = buildWrapTransaction("正文", EditorSelection.cursor(1), "*", "*", "input.format.italic");
    expect(emptySpec).toMatchObject({ changes: { insert: "**" }, selection: { anchor: 2 } });
  });
});
