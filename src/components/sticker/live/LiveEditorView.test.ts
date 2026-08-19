import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { createLiveView, setLiveDoc, setLiveFontFamily, setLiveFontSize } from "./LiveEditorView";
import { mathInstancePromise } from "../../../utils/markdown";
import { MathBlockWidget } from "./liveWidgets";

// CM6 在 jsdom 中需要 ResizeObserver / rAF / DOMRect polyfill
class ResizeObserverMock {
  observe() {}
  unobserve() {}
  disconnect() {}
}
beforeEach(() => {
  (globalThis as unknown as { ResizeObserver?: unknown }).ResizeObserver =
    (globalThis as unknown as { ResizeObserver?: unknown }).ResizeObserver ?? ResizeObserverMock;
  if (!(globalThis as unknown as { requestAnimationFrame?: unknown }).requestAnimationFrame) {
    (globalThis as unknown as { requestAnimationFrame: (cb: FrameRequestCallback) => number }).requestAnimationFrame = (cb) =>
      window.setTimeout(() => cb(Date.now()), 0);
  }
  if (!Range.prototype.getClientRects) {
    Range.prototype.getClientRects = () => [] as unknown as DOMRectList;
  }
});

function mountHost(): HTMLElement {
  const host = document.createElement("div");
  document.body.appendChild(host);
  return host;
}

describe("LiveEditorView（CM6 内核）", () => {
  afterEach(() => {
    document.body.innerHTML = "";
  });

  it("创建实例并加载文档", () => {
    const host = mountHost();
    const view = createLiveView(host, {
      doc: "# 标题\n正文\n\n**加粗**\n[链接](https://example.com)",
      fontSize: 14,
      onDocChange: () => {},
      onSave: () => {},
    });
    expect(view.state.doc.toString()).toBe("# 标题\n正文\n\n**加粗**\n[链接](https://example.com)");
    view.dispatch({ selection: { anchor: 5 } });
    expect(host.querySelector(".live-render")).not.toBeNull();
    expect(host.querySelector(".cm-live-h1")).not.toBeNull();
    expect(host.querySelector(".live-strong")).not.toBeNull();
    expect(host.querySelector(".live-link")).not.toBeNull();
    view.destroy();
  });

  it("在真实编辑器中直接提供 fenced 代码块 decoration", () => {
    const host = mountHost();
    const source = "块外正文\n\n```rust\nfn main() {}\n```";
    let view: ReturnType<typeof createLiveView> | undefined;

    expect(() => {
      view = createLiveView(host, {
        doc: source,
        fontSize: 14,
        onDocChange: () => {},
        onSave: () => {},
      });
    }).not.toThrow();
    expect(host.querySelector(".live-code-block pre > code.language-rust")).not.toBeNull();
    view?.destroy();
  });

  it("标题语法高亮不显示下划线", () => {
    const host = mountHost();
    const view = createLiveView(host, {
      doc: "# 标题\n正文",
      fontSize: 14,
      onDocChange: () => {},
      onSave: () => {},
    });
    view.dispatch({ selection: { anchor: view.state.doc.line(2).from } });
    const heading = host.querySelector<HTMLElement>(".cm-live-h1");
    expect(heading).not.toBeNull();
    const headingLine = heading!.closest<HTMLElement>(".cm-line");
    expect(headingLine).not.toBeNull();
    const underlined = [headingLine!, ...headingLine!.querySelectorAll<HTMLElement>("*")]
      .filter((element) => getComputedStyle(element).textDecoration.includes("underline"));
    expect(underlined).toHaveLength(0);
    view.destroy();
  });

  it("MathJax 就绪后首次打开的公式自动渲染", async () => {
    const host = mountHost();
    const view = createLiveView(host, {
      doc: "公式 $E=mc^2$",
      fontSize: 14,
      onDocChange: () => {},
      onSave: () => {},
    });
    await mathInstancePromise;
    await new Promise<void>((resolve) => queueMicrotask(resolve));
    expect(host.querySelector(".live-math .math-inline")).not.toBeNull();
    view.destroy();
  });

  it("及时预览将块级公式替换为 MathJax 渲染结果", async () => {
    const host = mountHost();
    const view = createLiveView(host, {
      doc: "$$\nE=mc^2\n$$\n正文",
      fontSize: 14,
      onDocChange: () => {},
      onSave: () => {},
    });
    view.dispatch({ selection: { anchor: view.state.doc.length } });
    await mathInstancePromise;
    await new Promise<void>((resolve) => queueMicrotask(resolve));
    expect(host.querySelector(".live-math-block .math-block")).not.toBeNull();
    view.destroy();
  });

  it("MathJax 首次就绪后会替换预初始化公式部件，而不会复用旧 DOM", () => {
    const source = "$$\nE=mc^2\n$$";
    expect(new MathBlockWidget(source, 0).eq(new MathBlockWidget(source, 1))).toBe(false);
  });

  it("及时预览将 Todo 标签替换为任务卡片", () => {
    const host = mountHost();
    const view = createLiveView(host, {
      doc: '<todo-block id="t-1"></todo-block>\n正文',
      fontSize: 14,
      todoBlocks: [{
        id: "t-1", sticker_id: 7, title: "购买牛奶", description: null, is_completed: false,
        parent_id: null, reminder_at: null, due_at: null, repeat_rule: null, created_at: "", updated_at: "",
      }],
      onDocChange: () => {},
      onSave: () => {},
    });
    view.dispatch({ selection: { anchor: view.state.doc.length } });
    expect(host.querySelector(".live-todo-block .todo-block-card")).not.toBeNull();
    expect(host.textContent).toContain("购买牛奶");
    view.destroy();
  });

  it("点击及时预览中的 Todo 卡片会按 data-todo-id 打开对应窗口", () => {
    const host = mountHost();
    const onTodoOpen = vi.fn();
    const view = createLiveView(host, {
      doc: '<todo-block id="t-1"></todo-block>\n正文',
      fontSize: 14,
      todoBlocks: [{
        id: "t-1", sticker_id: 7, title: "购买牛奶", description: null, is_completed: false,
        parent_id: null, reminder_at: null, due_at: null, repeat_rule: null, created_at: "", updated_at: "",
      }],
      onDocChange: () => {},
      onSave: () => {},
      onTodoOpen,
    });
    view.dispatch({ selection: { anchor: view.state.doc.length } });
    host.querySelector<HTMLElement>(".todo-block-card")?.click();
    expect(onTodoOpen).toHaveBeenCalledWith("t-1");
    view.destroy();
  });


  it("键盘回车走 Live Preview transaction 而不是插入裸换行", () => {
    const host = mountHost();
    const view = createLiveView(host, {
      doc: "- 项目",
      fontSize: 14,
      onDocChange: () => {},
      onSave: () => {},
    });
    view.dispatch({ selection: { anchor: view.state.doc.length } });
    view.contentDOM.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    expect(view.state.doc.toString()).toBe("- 项目\n- ");
    view.destroy();
  });

  it("点击任务 checkbox 修改它对应的源行，而不是当前光标行", () => {
    const host = mountHost();
    const view = createLiveView(host, {
      doc: "- [ ] 第一项\n- [ ] 第二项\n普通文本",
      fontSize: 14,
      onDocChange: () => {},
      onSave: () => {},
    });
    view.dispatch({ selection: { anchor: view.state.doc.length } });
    const checkbox = host.querySelector<HTMLInputElement>(".live-task-checkbox");
    expect(checkbox).not.toBeNull();
    checkbox!.click();
    expect(view.state.doc.toString()).toBe("- [x] 第一项\n- [ ] 第二项\n普通文本");
    view.destroy();
  });

  it("用户编辑（dispatch）触发 onDocChange", () => {
    const host = mountHost();
    const onChange = vi.fn();
    const view = createLiveView(host, {
      doc: "abc",
      fontSize: 14,
      onDocChange: onChange,
      onSave: () => {},
    });
    view.dispatch({ changes: { from: 3, insert: "!" } });
    expect(onChange).toHaveBeenCalledWith("abc!");
    view.destroy();
  });

  it("setLiveDoc 外部同步（内容相同跳过 dispatch）", () => {
    const host = mountHost();
    const onChange = vi.fn();
    const view = createLiveView(host, {
      doc: "abc",
      fontSize: 14,
      onDocChange: onChange,
      onSave: () => {},
    });
    setLiveDoc(view, "xyz");
    expect(view.state.doc.toString()).toBe("xyz");
    // CM6 语义：任何 dispatch（含外部同步）都触发 updateListener；
    // 内容相同去重与回写保护由调用方（组件 scheduleEmit 按 props 比对）负责
    const callsBefore = onChange.mock.calls.length;
    setLiveDoc(view, "xyz"); // 相同内容：不 dispatch
    expect(view.state.doc.toString()).toBe("xyz");
    expect(onChange.mock.calls.length).toBe(callsBefore);
    view.destroy();
  });

  it("setLiveFontSize 热替换不抛错", () => {
    const host = mountHost();
    const view = createLiveView(host, {
      doc: "text",
      fontSize: 14,
      onDocChange: () => {},
      onSave: () => {},
    });
    expect(() => setLiveFontSize(view, 20)).not.toThrow();
    view.destroy();
  });

  it("setLiveFontFamily 热替换不抛错", () => {
    const host = mountHost();
    const view = createLiveView(host, {
      doc: "text",
      fontSize: 14,
      fontFamily: "Microsoft YaHei",
      onDocChange: () => {},
      onSave: () => {},
    });
    expect(() => setLiveFontFamily(view, "Segoe UI")).not.toThrow();
    view.destroy();
  });
});
