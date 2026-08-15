import { describe, it, expect } from "vitest";
import {
  handleEnterAtCursor,
  handleTabAtCursor,
  handleShiftTabAtCursor,
} from "./edit-actions";

describe("handleEnterAtCursor（回车智能续行）", () => {
  it("无序列表续行带 `- `", () => {
    const text = "- 文本";
    const r = handleEnterAtCursor(text, text.length)!;
    expect(r.text).toBe("- 文本\n- ");
    expect(r.cursor).toBe(r.text.length);
  });

  it("空列表项回车退出列表", () => {
    const r = handleEnterAtCursor("- ", 2)!;
    expect(r.text).toBe("");
    expect(r.cursor).toBe(0);
  });

  it("有序列表续行编号递增（1. → 2.）", () => {
    const t1 = "3. 第三项";
    const r1 = handleEnterAtCursor(t1, t1.length)!;
    expect(r1.text).toBe("3. 第三项\n4. ");
    const r2 = handleEnterAtCursor(r1.text, r1.cursor)!;
    expect(r2.text).toBe("3. 第三项\n");
  });

  it("任务列表续行为未勾选 `- [ ] `", () => {
    const t1 = "- [x] 已完成";
    const r1 = handleEnterAtCursor(t1, t1.length)!;
    expect(r1.text).toBe("- [x] 已完成\n- [ ] ");
    const r2 = handleEnterAtCursor(r1.text, r1.cursor)!;
    expect(r2.text).toBe("- [x] 已完成\n");
  });

  it("嵌套列表保留缩进", () => {
    const text = "  - 子项";
    const r = handleEnterAtCursor(text, text.length)!;
    expect(r.text).toBe("  - 子项\n  - ");
  });

  it("复合编号续行递增（1.1 → 1.2）", () => {
    const text = "1. 文本\n  1.1 文本";
    const r = handleEnterAtCursor(text, text.length)!;
    expect(r.text).toBe("1. 文本\n  1.1 文本\n  1.2 ");
  });

  it("光标在行首：上方插入空列表项", () => {
    const r = handleEnterAtCursor("- 文本", 0)!;
    expect(r.text).toBe("- \n- 文本");
    expect(r.cursor).toBe(2);
  });

  it("普通行返回 null（默认回车）", () => {
    expect(handleEnterAtCursor("普通段落", 6)).toBeNull();
    expect(handleEnterAtCursor("# 标题", 5)).toBeNull();
  });

  it("围栏代码块内返回 null", () => {
    const text = "```\n- [ ] 代码示例\n```";
    const cursor = text.indexOf("- [ ] 代码示例") + "- [ ] 代码示例".length;
    expect(handleEnterAtCursor(text, cursor)).toBeNull();
  });
});

describe("handleTabAtCursor（Tab 智能缩进）", () => {
  it("行首继承无序列表生成嵌套子项", () => {
    const text = "- 这里是文本\n";
    const r = handleTabAtCursor(text, text.length);
    expect(r.text).toBe("- 这里是文本\n  - ");
    expect(r.cursor).toBe(r.text.length);
  });

  it("任务列表继承 `  - [ ] `", () => {
    const text = "- [ ] 买牛奶\n";
    const r = handleTabAtCursor(text, text.length);
    expect(r.text).toBe("- [ ] 买牛奶\n  - [ ] ");
  });

  it("有序列表继承嵌套从 1 重排", () => {
    const text = "3. 第三项\n";
    const r = handleTabAtCursor(text, text.length);
    expect(r.text).toBe("3. 第三项\n  1. ");
  });

  it("有序行内 Tab 生成复合编号（2. → 1.1）", () => {
    const text = "1. 文本\n2. ";
    const r = handleTabAtCursor(text, text.length);
    expect(r.text).toBe("1. 文本\n  1.1 ");
  });

  it("上一行普通文本：插入 2 空格", () => {
    const text = "普通段落\n";
    const r = handleTabAtCursor(text, text.length);
    expect(r.text).toBe("普通段落\n  ");
  });

  it("围栏内退化为普通 2 空格缩进", () => {
    const text = "```\ncode\n```";
    const cursor = text.indexOf("code") + 4;
    const r = handleTabAtCursor(text, cursor);
    expect(r.text).toBe("```\n  code\n```");
  });
});

describe("handleShiftTabAtCursor（Shift+Tab）", () => {
  it("无序列表去掉 2 空格缩进", () => {
    const text = "  - 子项";
    const r = handleShiftTabAtCursor(text, text.length)!;
    expect(r.text).toBe("- 子项");
  });

  it("有序复合编号提升一级（1.1 → 2.，无缩进）", () => {
    const text = "1. 文本\n  1.1 文本";
    const cursor = text.indexOf("1.1") + 3;
    const r = handleShiftTabAtCursor(text, cursor)!;
    expect(r.text).toBe("1. 文本\n2. 文本");
  });

  it("顶层有序/普通行无效果", () => {
    expect(handleShiftTabAtCursor("1. 文本", 4)).toBeNull();
    expect(handleShiftTabAtCursor("普通", 2)).toBeNull();
  });
});
