import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { createLiveView, setLiveDoc, setLiveFontSize } from "./LiveEditorView";

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
      doc: "# 标题\n正文",
      fontSize: 14,
      onDocChange: () => {},
      onSave: () => {},
    });
    expect(view.state.doc.toString()).toBe("# 标题\n正文");
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
});
