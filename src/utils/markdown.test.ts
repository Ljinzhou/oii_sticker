import { describe, it, expect } from "vitest";
import { renderMarkdown, hexToRgba, mathInstancePromise } from "./markdown";

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
