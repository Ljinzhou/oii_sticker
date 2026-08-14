import { describe, it, expect } from "vitest";
import { highlightMarkdown } from "./markdown-highlight";

describe("highlightMarkdown", () => {
  it("围栏内按语言高亮（js 关键字着色）", () => {
    const out = highlightMarkdown("```js\nconst a = 1;\n```");
    expect(out).toContain("hljs-keyword");
    expect(out).toContain("const");
  });

  it("围栏外普通文本转义", () => {
    const out = highlightMarkdown("<div> & 文本");
    expect(out).toContain("&lt;div&gt;");
    expect(out).not.toContain("<div>");
  });

  it("无语言围栏不高亮（原样转义）", () => {
    const out = highlightMarkdown("```\nconst a = 1;\n```");
    expect(out).not.toContain("hljs-keyword");
    expect(out).toContain("const a = 1;");
  });

  it("围栏开关行本身原样保留", () => {
    const out = highlightMarkdown("```python\nx = 1\n```");
    expect(out).toContain("```python");
    expect(out).toContain("hljs-number"); // python 数字着色
  });

  it("多围栏交替识别", () => {
    const out = highlightMarkdown("a\n```js\nlet x;\n```\nb\n```js\nlet y;\n```");
    const hlCount = out.split("hljs-keyword").length - 1;
    expect(hlCount).toBe(2);
  });

  it("markdown 语法标记着色（标题/列表/粗体/行内代码）", () => {
    const out = highlightMarkdown("# 标题\n- [ ] 任务\n**加粗** 与 `代码` 和 $x^2$");
    expect(out).toContain('<span class="md-head">#</span>');
    expect(out).toContain('<span class="md-task">[ ]</span>');
    expect(out).toContain('<span class="md-strong">**</span>');
    expect(out).toContain('<span class="md-code">`代码`</span>');
    expect(out).toContain('<span class="md-math">$</span>');
    // 内容本身保持默认色（未被包进标记 span）
    expect(out).toContain("加粗");
  });

  it("语法着色不破坏转义（html 实体安全）", () => {
    const out = highlightMarkdown("## 标题 <a> & b");
    expect(out).toContain("&lt;a&gt;");
    expect(out).toContain('<span class="md-head">##</span>');
  });

  it("有序列表与引用/分隔线着色", () => {
    const out = highlightMarkdown("1. 第一\n> 引用\n---");
    expect(out).toContain('<span class="md-list">1.</span>');
    expect(out).toContain('<span class="md-quote">&gt;</span>');
    expect(out).toContain('<span class="md-hr">---</span>');
  });

  it("围栏内代码不被 markdown 语法着色干扰", () => {
    const out = highlightMarkdown("```js\n# 不是标题\nconst x = 1;\n```");
    // 围栏内行交给 hljs：js 中 # 开头是注释或非法，不产生 md-head span
    expect(out).not.toContain("md-head");
    expect(out).toContain("hljs-keyword");
  });
});
