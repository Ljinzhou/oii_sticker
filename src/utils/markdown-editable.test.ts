import { describe, it, expect } from "vitest";
import { renderMarkdownEditable, htmlToMarkdown } from "./markdown-editable";
import { mathInstancePromise } from "./markdown";

describe("renderMarkdownEditable", () => {
  it("任务标记保留为 [ ] 文本（不生成 checkbox）", () => {
    const html = renderMarkdownEditable("- [ ] 待办\n- [x] 完成");
    expect(html).toContain("[ ] 待办");
    expect(html).not.toContain("task-checkbox");
  });
});

describe("htmlToMarkdown", () => {
  it("基础 HTML 回写 Markdown（粗体/标题）", () => {
    const md = htmlToMarkdown("<h1>标题</h1><p><strong>粗</strong>文本</p>");
    expect(md).toContain("# 标题");
    expect(md).toContain("**粗**");
  });

  it("数学公式经 data-tex 保真回写（$..$ 与 $$..$$）", async () => {
    await mathInstancePromise;
    const html = renderMarkdownEditable("内联 $E=mc^2$ 与块级 $$\n\\frac{1}{2}\n$$");
    const md = htmlToMarkdown(html);
    expect(md).toContain("$E=mc^2$");
    expect(md).toContain("$$");
    expect(md).toContain("\\frac{1}{2}");
  });
});
