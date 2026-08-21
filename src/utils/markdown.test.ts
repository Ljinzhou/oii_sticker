import { describe, it, expect } from "vitest";
import { renderMarkdown, hexToRgba, mathInstancePromise, normalizeCompoundLists } from "./markdown";
import type { TodoBlock } from "../types";

describe("renderMarkdown", () => {
  it("渲染基础 Markdown（标题/粗体/列表）", () => {
    const html = renderMarkdown("# 标题\n\n**加粗** 与 `代码`");
    expect(html).toContain("<h1>标题</h1>");
    expect(html).toContain("<strong>加粗</strong>");
    expect(html).toContain("<code>代码</code>");
  });

  it("任务清单 checkbox 带 data-line 源行号", () => {
    const content = "- [ ] 任务一\n- [x] 任务二";
    const html = renderMarkdown(content);
    // 第 0 行未勾选
    expect(html).toContain(`data-line="0"`);
    expect(html).not.toContain(`data-line="0" checked`);
    // 第 1 行已勾选
    expect(html).toContain(`data-line="1" checked`);
  });

  it("普通列表不生成 checkbox", () => {
    const html = renderMarkdown("- 普通项");
    expect(html).not.toContain("task-checkbox");
  });

  it("引用块与有序列表渲染", () => {
    const html = renderMarkdown("> 引用\n\n1. 第一\n2. 第二");
    expect(html).toContain("<blockquote>");
    expect(html).toContain("<ol>");
    expect(html).toContain("第一");
  });

  it("数学公式 $..$ 渲染为 math-inline 容器（含真实 SVG 输出）", async () => {
    await mathInstancePromise;
    const html = renderMarkdown("公式 $E=mc^2$ 测试");
    expect(html).toContain("math-inline");
    expect(html).toContain("data-tex=");
    expect(html).toContain("E=mc^2");
    // 字体预加载生效：SVG 实际输出（mjx-container 内包含 <svg> 与字形 path）
    expect(html).toContain("mjx-container");
    expect(html).toContain("<svg");
    expect(html).toContain('d="');
  });

  it("块级公式 $$..$$ 渲染为 math-block 容器", async () => {
    await mathInstancePromise;
    const html = renderMarkdown("$$\n\\frac{1}{2}\n$$");
    expect(html).toContain("math-block");
  });

  it("受控 Todo 标记渲染为卡片，任意 HTML 不会透传", () => {
    const todo: TodoBlock = { id: "t-1", sticker_id: 1, title: "购物", block_title: "", description: null, is_completed: false, parent_id: null, reminder_at: null, due_at: null, repeat_rule: null, created_at: "", updated_at: "" };
    const html = renderMarkdown('<todo-block id="t-1"></todo-block>\n\n<script>alert(1)</script>', [todo]);
    expect(html).toContain('data-todo-id="t-1"');
    expect(html).toContain("购物");
    expect(html).not.toContain("<script>");
  });

  it("同便签多个标记合并为一张卡：全部任务完整显示、后续标记不重复渲染", () => {
    const rootA: TodoBlock = { id: "t-1", sticker_id: 1, title: "任务一", block_title: "任务块", description: null, is_completed: false, parent_id: null, reminder_at: null, due_at: null, repeat_rule: null, created_at: "", updated_at: "" };
    const childA: TodoBlock = { ...rootA, id: "t-2", title: "子任务1", parent_id: "t-1" };
    const rootB: TodoBlock = { ...rootA, id: "t-3", title: "任务二" };
    const html = renderMarkdown('<todo-block id="t-1"></todo-block>\n\n<todo-block id="t-3"></todo-block>', [rootA, childA, rootB]);
    expect((html.match(/todo-block-card/g) ?? []).length).toBe(1);
    expect(html).toContain("任务一");
    expect(html).toContain("子任务1");
    expect(html).toContain("任务二");
    expect(html).toContain("任务块");
    expect(html).toContain("0 / 3");
    // 校验框只渲染每个任务一次
    expect((html.match(/todo-task-checkbox/g) ?? []).length).toBe(3);
  });
});

describe("normalizeCompoundLists", () => {
  it("复合编号行转为嵌套列表语法", () => {
    expect(normalizeCompoundLists("1. 文本\n1.1 子项")).toBe("1. 文本\n  1. 子项");
    expect(normalizeCompoundLists("1. 文本\n1.1 子项\n1.1.1 深层")).toBe(
      "1. 文本\n  1. 子项\n    1. 深层",
    );
    expect(normalizeCompoundLists("1.1. 尾点文本")).toBe("  1. 尾点文本");
  });

  it("单级编号与普通行不变", () => {
    expect(normalizeCompoundLists("1. 文本\n2. 文本\n普通行")).toBe("1. 文本\n2. 文本\n普通行");
  });

  it("围栏代码块内不转换", () => {
    const md = "```\n1.1 code line\n```";
    expect(normalizeCompoundLists(md)).toBe(md);
  });
});

describe("hexToRgba", () => {
  it("转换 hex 为 rgba", () => {
    expect(hexToRgba("#FF5733", 0.5)).toBe("rgba(255, 87, 51, 0.5)");
    expect(hexToRgba("FF5733", 1)).toBe("rgba(255, 87, 51, 1)");
  });

  it("非法输入回退默认色", () => {
    expect(hexToRgba("bad", 0.5)).toBe("rgba(255, 255, 255, 0.5)");
  });
});
