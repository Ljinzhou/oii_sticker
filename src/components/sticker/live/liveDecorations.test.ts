import { describe, it, expect, beforeEach } from "vitest";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { collectInlineRanges, buildLiveDecorations } from "./liveDecorations";

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
});
