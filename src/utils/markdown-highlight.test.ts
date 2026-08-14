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
});
