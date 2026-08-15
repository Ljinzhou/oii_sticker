import { describe, it, expect, beforeEach } from "vitest";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { collectInlineRanges, collectBlockRanges, buildLiveDecorations } from "./liveDecorations";

// jsdom polyfill（CM6 需要）
class ResizeObserverMock {
  observe() {}
  unobserve() {}
  disconnect() {}
}
beforeEach(() => {
  (globalThis as unknown as { ResizeObserver?: unknown }).ResizeObserver =
    (globalThis as unknown as { ResizeObserver?: unknown }).ResizeObserver ?? ResizeObserverMock;
  document.body.innerHTML = "";
});

function makeView(doc: string, cursorPos?: number): EditorView {
  const state = EditorState.create({
    doc,
    extensions: [markdown({ base: markdownLanguage })],
  });
  const host = document.createElement("div");
  document.body.appendChild(host);
  const view = new EditorView({ state, parent: host });
  if (cursorPos !== undefined) {
    view.dispatch({ selection: { anchor: cursorPos } });
  }
  return view;
}

describe("collectInlineRanges", () => {
  it("识别粗体/斜体/删除线/行内代码/链接", () => {
    const view = makeView("**粗** *斜* ~~删~~ `码` [链](https://x)");
    const ranges = collectInlineRanges(view);
    expect(ranges.filter((r) => r.kind === "render").length).toBe(5);
    const link = ranges.find((r) => view.state.doc.sliceString(r.from, r.to).startsWith("["));
    expect(link).toBeTruthy();
    view.destroy();
  });

  it("任务标记识别 checked 状态", () => {
    const view = makeView("- [ ] 待办\n- [x] 完成");
    const tasks = collectInlineRanges(view).filter((r) => r.kind === "task");
    expect(tasks.length).toBe(2);
    expect(tasks[0].checked).toBe(false);
    expect(tasks[1].checked).toBe(true);
    view.destroy();
  });

  it("嵌套只保留最外层（非重叠贪心）", () => {
    const view = makeView("**粗 *斜* 体**");
    const renders = collectInlineRanges(view).filter((r) => r.kind === "render");
    expect(renders.length).toBe(1);
    expect(view.state.doc.sliceString(renders[0].from, renders[0].to)).toBe("**粗 *斜* 体**");
    view.destroy();
  });

  it("数学公式识别且代码块内排除", () => {
    const view = makeView("公式 $x^2$ 测试\n\n```\n$not math$\n```");
    const math = collectInlineRanges(view).filter((r) => r.kind === "math");
    expect(math.length).toBe(1);
    expect(view.state.doc.sliceString(math[0].from, math[0].to)).toBe("$x^2$");
    view.destroy();
  });

  it("代码块内容不产生行内渲染范围", () => {
    const view = makeView("```js\n**不是粗体**\n```");
    const renders = collectInlineRanges(view).filter((r) => r.kind === "render");
    expect(renders.length).toBe(0);
    view.destroy();
  });
});

describe("collectBlockRanges（块级渲染）", () => {
  it("标题标记与标题行范围（含级别）", () => {
    const view = makeView("# 一级\n## 二级");
    const blocks = collectBlockRanges(view);
    const marks = blocks.filter((b) => b.kind === "heading-mark");
    expect(marks.map((m) => m.level)).toEqual([1, 2]);
    const lines = blocks.filter((b) => b.kind === "heading-line");
    expect(lines.length).toBe(2);
    view.destroy();
  });

  it("无序列表标记渲染为圆点", () => {
    const view = makeView("- 一项\n- 二项");
    const marks = collectBlockRanges(view).filter((b) => b.kind === "listmark");
    expect(marks.length).toBe(2);
    expect(marks.every((m) => m.ordinal === "•")).toBe(true);
    view.destroy();
  });

  it("有序列表嵌套编号（1. / 1.1. / 1.1.1.）", () => {
    const view = makeView("1. a\n   1. b\n      1. c");
    const marks = collectBlockRanges(view).filter((b) => b.kind === "listmark");
    expect(marks.map((m) => m.ordinal)).toEqual(["1.", "1.1.", "1.1.1."]);
    view.destroy();
  });

  it("有序列表同层续接编号（1. / 2.）", () => {
    const view = makeView("1. a\n2. b");
    const marks = collectBlockRanges(view).filter((b) => b.kind === "listmark");
    expect(marks.map((m) => m.ordinal)).toEqual(["1.", "2."]);
    view.destroy();
  });

  it("复合编号行识别为列表标记（含缩进 mark）", () => {
    const view = makeView("1. a\n  1.1 b\n    3.1.1 c\n1. 尾\n1.1. 尾点");
    const blocks = collectBlockRanges(view).sort((a, b) => a.from - b.from);
    // lezer 单级列表（1. 与同列表第 2 项 2.）+ 复合编号（1.1 / 3.1.1 / 1.1.）
    const marks = blocks.filter((b) => b.kind === "listmark");
    expect(marks.map((m) => m.ordinal)).toEqual(["1.", "1.1", "3.1.1", "2.", "1.1."]);
    const indents = blocks.filter((b) => b.kind === "compound-line");
    expect(indents.map((i) => i.level)).toEqual([1, 2, 1]);
    view.destroy();
  });

  it("围栏代码块内的复合编号不识别", () => {
    const view = makeView("```\n1.1 code\n```");
    const marks = collectBlockRanges(view).filter((b) => b.kind === "listmark");
    expect(marks.length).toBe(0);
    view.destroy();
  });

  it("引用标记与引用行范围、分隔线", () => {
    const view = makeView("> 引用\n\n---");
    const blocks = collectBlockRanges(view);
    expect(blocks.some((b) => b.kind === "quote")).toBe(true);
    expect(blocks.some((b) => b.kind === "quote-line")).toBe(true);
    expect(blocks.some((b) => b.kind === "hr")).toBe(true);
    view.destroy();
  });
});

describe("buildLiveDecorations", () => {
  it("光标所在行的元素不渲染（显示源码），其他行渲染", () => {
    const view = makeView("**行一**\n**行二**", 0); // 光标在行一开头
    const deco = buildLiveDecorations(view);
    let count = 0;
    deco.between(0, view.state.doc.length, () => {
      count++;
    });
    expect(count).toBe(1); // 只有行二渲染
    view.destroy();
  });

  it("光标移动后重算（行二有光标时行二不渲染）", () => {
    const view = makeView("**行一**\n**行二**", 0);
    view.dispatch({ selection: { anchor: view.state.doc.line(2).from } });
    const deco = buildLiveDecorations(view);
    let count = 0;
    deco.between(0, view.state.doc.length, () => {
      count++;
    });
    expect(count).toBe(1); // 只有行一渲染
    view.destroy();
  });

  it("Obsidian 精确行为：光标在行内但不在元素内时，该元素仍渲染", () => {
    const view = makeView("abc **粗体**", 0); // 光标在行首普通文本
    const deco = buildLiveDecorations(view);
    let count = 0;
    deco.between(0, view.state.doc.length, () => {
      count++;
    });
    expect(count).toBe(1); // strong 仍渲染（光标不在其内）
    view.destroy();
  });

  it("Obsidian 精确行为：光标进入元素（含边界）→ 显示源码", () => {
    const view = makeView("abc **粗体**", 0);
    view.dispatch({ selection: { anchor: 5 } }); // 光标在 ** 起始边界
    const deco = buildLiveDecorations(view);
    let count = 0;
    deco.between(0, view.state.doc.length, () => {
      count++;
    });
    expect(count).toBe(0); // strong 显示源码
    view.destroy();
  });

  it("选区跨越元素 → 显示源码", () => {
    const view = makeView("abc **粗体** 结尾", 0);
    view.dispatch({ selection: { anchor: 0, head: 8 } }); // 选中跨越 strong
    const deco = buildLiveDecorations(view);
    let count = 0;
    deco.between(0, view.state.doc.length, () => {
      count++;
    });
    expect(count).toBe(0);
    view.destroy();
  });
});
