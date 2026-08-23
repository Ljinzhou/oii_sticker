import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import {
  createMarkdownSourceView,
  setMarkdownSourceDoc,
  setMarkdownSourceFontFamily,
  setMarkdownSourceFontSize,
  setMarkdownSourceLineNumbers,
} from "./sourceEditor";

// CM6 在 jsdom 中需要 ResizeObserver / rAF / DOMRect polyfill（与 LiveEditorView.test 一致）
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

describe("Markdown 源码编辑器（sourceEditor）", () => {
  afterEach(() => {
    document.body.innerHTML = "";
  });

  it("showLineNumbers=true 时渲染与及时预览相同的 CodeMirror 行号区 + 折叠小三角", () => {
    const host = mountHost();
    const view = createMarkdownSourceView(host, {
      doc: "第一行\n第二行",
      fontSize: 14,
      showLineNumbers: true,
      onDocChange: () => {},
    });
    expect(host.querySelector(".cm-gutters .cm-lineNumbers")).not.toBeNull();
    expect(host.querySelector(".cm-foldGutter")).not.toBeNull();
    view.destroy();
  });

  it("showLineNumbers=false 时不渲染行号区（含折叠箭头），setMarkdownSourceLineNumbers 可热切换", () => {
    const host = mountHost();
    const view = createMarkdownSourceView(host, {
      doc: "第一行\n第二行",
      fontSize: 14,
      showLineNumbers: false,
      onDocChange: () => {},
    });
    expect(host.querySelector(".cm-gutters")).toBeNull();
    setMarkdownSourceLineNumbers(view, true);
    expect(host.querySelector(".cm-gutters .cm-lineNumbers")).not.toBeNull();
    expect(host.querySelector(".cm-foldGutter")).not.toBeNull();
    setMarkdownSourceLineNumbers(view, false);
    expect(host.querySelector(".cm-gutters")).toBeNull();
    view.destroy();
  });

  it("复用 liveTransforms：列表行回车自动续行", () => {
    const host = mountHost();
    const view = createMarkdownSourceView(host, {
      doc: "- 项目",
      fontSize: 14,
      showLineNumbers: false,
      onDocChange: () => {},
    });
    view.dispatch({ selection: { anchor: view.state.doc.length } });
    view.contentDOM.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    expect(view.state.doc.toString()).toBe("- 项目\n- ");
    view.destroy();
  });

  it("用户编辑（dispatch）触发 onDocChange", () => {
    const host = mountHost();
    const onChange = vi.fn();
    const view = createMarkdownSourceView(host, {
      doc: "abc",
      fontSize: 14,
      showLineNumbers: true,
      onDocChange: onChange,
    });
    view.dispatch({ changes: { from: 3, insert: "!" } });
    expect(onChange).toHaveBeenCalledWith("abc!");
    view.destroy();
  });

  it("setMarkdownSourceDoc 外部同步（内容相同跳过 dispatch）", () => {
    const host = mountHost();
    const onChange = vi.fn();
    const view = createMarkdownSourceView(host, {
      doc: "abc",
      fontSize: 14,
      showLineNumbers: true,
      onDocChange: onChange,
    });
    setMarkdownSourceDoc(view, "xyz");
    expect(view.state.doc.toString()).toBe("xyz");
    const callsBefore = onChange.mock.calls.length;
    setMarkdownSourceDoc(view, "xyz");
    expect(view.state.doc.toString()).toBe("xyz");
    expect(onChange.mock.calls.length).toBe(callsBefore);
    view.destroy();
  });

  it("setMarkdownSourceFontSize / setMarkdownSourceFontFamily 热替换不抛错", () => {
    const host = mountHost();
    const view = createMarkdownSourceView(host, {
      doc: "text",
      fontSize: 14,
      showLineNumbers: true,
      onDocChange: () => {},
    });
    expect(() => setMarkdownSourceFontSize(view, 18)).not.toThrow();
    expect(() => setMarkdownSourceFontFamily(view, "Segoe UI")).not.toThrow();
    view.destroy();
  });

  it("斜杠菜单打开时方向键/回车/Esc 交给菜单：不移动光标、不换行", () => {
    const host = mountHost();
    const onNav = vi.fn();
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    let open = true;
    const view = createMarkdownSourceView(host, {
      doc: "- 项目",
      fontSize: 14,
      showLineNumbers: false,
      onDocChange: () => {},
      slashOpen: () => open,
      onSlashNav: onNav,
      onSlashConfirm: onConfirm,
      onSlashCancel: onCancel,
    });
    view.dispatch({ selection: { anchor: view.state.doc.length } });
    const head = view.state.selection.main.head;

    const keydown = (key: string) =>
      new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true });
    view.contentDOM.dispatchEvent(keydown("ArrowDown"));
    expect(onNav).toHaveBeenCalledWith(1);
    view.contentDOM.dispatchEvent(keydown("ArrowUp"));
    expect(onNav).toHaveBeenCalledWith(-1);
    view.contentDOM.dispatchEvent(keydown("Enter"));
    expect(onConfirm).toHaveBeenCalledTimes(1);
    view.contentDOM.dispatchEvent(keydown("Escape"));
    expect(onCancel).toHaveBeenCalledTimes(1);
    // 光标没有移动、内容没有换行
    expect(view.state.selection.main.head).toBe(head);
    expect(view.state.doc.toString()).toBe("- 项目");

    // 菜单关闭后按键回到正常编辑行为
    open = false;
    onNav.mockClear();
    onConfirm.mockClear();
    view.contentDOM.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true, cancelable: true }));
    expect(onNav).not.toHaveBeenCalled();
    // 菜单关闭时回车恢复列表续行
    view.contentDOM.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
    expect(view.state.doc.toString()).toBe("- 项目\n- ");
    view.destroy();
  });

  it("多行选区 Tab 整体缩进（每行前加 2 空格）", () => {
    const host = mountHost();
    const view = createMarkdownSourceView(host, {
      doc: "a\nb\nc",
      fontSize: 14,
      showLineNumbers: false,
      onDocChange: () => {},
    });
    // 选中第二行行首到第三行行首（"b\nc"）
    view.dispatch({ selection: { anchor: 2, head: 4 } });
    view.contentDOM.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
    expect(view.state.doc.toString()).toBe("a\n  b\n  c");
    view.destroy();
  });
});
