import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { createLiveView, setLiveDoc, setLiveFontFamily, setLiveFontSize, setLiveLineNumbers } from "./LiveEditorView";
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
      // 三层：块 t-1（自身不是任务）→ 父任务 t-2（显示任务名）
      todoBlocks: [
        { id: "t-1", sticker_id: 7, title: "", block_title: "购物块", description: null, is_completed: false, parent_id: null, reminder_at: null, due_at: null, repeat_rule: null, created_at: "", updated_at: "" },
        { id: "t-2", sticker_id: 7, title: "购买牛奶", block_title: "", description: null, is_completed: false, parent_id: "t-1", reminder_at: null, due_at: null, repeat_rule: null, created_at: "", updated_at: "" },
      ],
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
        id: "t-1", sticker_id: 7, title: "购买牛奶", block_title: "", description: null, is_completed: false,
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
  it("点击卡片内的折叠箭头只切换显隐，不触发打开窗口", () => {
    const host = mountHost();
    const onTodoOpen = vi.fn();
    const onBlockUiAction = vi.fn();
    const view = createLiveView(host, {
      doc: '<todo-block id="t-1"></todo-block>\n正文',
      fontSize: 14,
      todoBlocks: [{
        id: "t-1", sticker_id: 7, title: "任务", block_title: "", description: null, is_completed: false,
        parent_id: null, reminder_at: null, due_at: null, repeat_rule: null, created_at: "", updated_at: "",
      }],
      onDocChange: () => {},
      onSave: () => {},
      onTodoOpen,
      onBlockUiAction,
    });
    view.dispatch({ selection: { anchor: view.state.doc.length } });
    const caret = host.querySelector<HTMLElement>("[data-fold]");
    expect(caret).not.toBeNull();
    caret?.click();
    expect(onBlockUiAction).toHaveBeenCalledWith("foldCard", "t-1");
    expect(onTodoOpen).not.toHaveBeenCalled();
    view.destroy();
  });

  it("Todo 标签行退格两次即可整块删除（先删行内容留空行，再并掉空行）", () => {
    const host = mountHost();
    const view = createLiveView(host, {
      doc: '<todo-block id="t-1"></todo-block>\n正文',
      fontSize: 14,
      todoBlocks: [{
        id: "t-1", sticker_id: 7, title: "购买牛奶", block_title: "", description: null, is_completed: false,
        parent_id: null, reminder_at: null, due_at: null, repeat_rule: null, created_at: "", updated_at: "",
      }],
      onDocChange: () => {},
      onSave: () => {},
    });
    // 光标放在标签行首（点击卡片后的自然落点：原子区间的边缘）
    const tagEnd = '<todo-block id="t-1"></todo-block>'.length;
    expect(tagEnd).toBeGreaterThan(0);
    view.dispatch({ selection: { anchor: 0 } });
    expect(view.state.selection.main.head).toBe(0);
    const pressBackspace = () =>
      view.contentDOM.dispatchEvent(new KeyboardEvent("keydown", { key: "Backspace", bubbles: true }));
    pressBackspace();
    expect(view.state.doc.toString()).toBe("\n正文");
    // 光标在下一行（正文）行首：第二次退格并掉空行
    expect(view.state.selection.main.head).toBe(1);
    pressBackspace();
    expect(view.state.doc.toString()).toBe("正文");
    view.destroy();
  });

  it("普通文本行的退格不受整行删除逻辑影响", () => {
    const host = mountHost();
    const view = createLiveView(host, {
      doc: "abc",
      fontSize: 14,
      onDocChange: () => {},
      onSave: () => {},
    });
    view.dispatch({ selection: { anchor: 3 } });
    view.contentDOM.dispatchEvent(new KeyboardEvent("keydown", { key: "Backspace", bubbles: true }));
    expect(view.state.doc.toString()).toBe("ab");
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

  it("showLineNumbers=false 隐藏行号，setLiveLineNumbers 热切换（与 Markdown 模式同一开关）", () => {
    const host = mountHost();
    const view = createLiveView(host, {
      doc: "第一行\n第二行",
      fontSize: 14,
      showLineNumbers: false,
      onDocChange: () => {},
      onSave: () => {},
    });
    expect(host.querySelector(".cm-gutters")).toBeNull();
    setLiveLineNumbers(view, true);
    expect(host.querySelector(".cm-gutters .cm-lineNumbers")).not.toBeNull();
    setLiveLineNumbers(view, false);
    expect(host.querySelector(".cm-gutters")).toBeNull();
    view.destroy();
  });

  it("未传 showLineNumbers 时默认显示行号（与旧行为一致）", () => {
    const host = mountHost();
    const view = createLiveView(host, {
      doc: "text",
      fontSize: 14,
      onDocChange: () => {},
      onSave: () => {},
    });
    expect(host.querySelector(".cm-gutters .cm-lineNumbers")).not.toBeNull();
    view.destroy();
  });

  it("斜杠菜单打开时 ↑/↓/Enter/Esc 交给菜单（列表行也不续行不移动光标）", () => {
    const host = mountHost();
    const onNav = vi.fn();
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    let open = true;
    const view = createLiveView(host, {
      doc: "- 项目",
      fontSize: 14,
      showLineNumbers: false,
      onDocChange: () => {},
      onSave: () => {},
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
    // 光标不移动、列表行不续行
    expect(view.state.selection.main.head).toBe(head);
    expect(view.state.doc.toString()).toBe("- 项目");

    // 菜单关闭后回车恢复列表续行（markdown 的 Enter 在高优先级键位放行后生效）
    open = false;
    onConfirm.mockClear();
    view.contentDOM.dispatchEvent(keydown("Enter"));
    expect(view.state.doc.toString()).toBe("- 项目\n- ");
    view.destroy();
  });
  it("粘贴链接自动转为 [](url) 占位，并异步用网页标题填充", async () => {
    const host = mountHost();
    const fetched: string[] = [];
    const view = createLiveView(host, {
      doc: "",
      fontSize: 14,
      onDocChange: () => {},
      onSave: () => {},
      fetchPageTitle: async (url) => {
        fetched.push(url);
        return "哔哩哔哩";
      },
    });
    view.dispatch({
      changes: { from: 0, insert: "https://www.bilibili.com/video/av1" },
      userEvent: "input.paste",
    });
    // 第一步：立即转成空标题占位
    expect(view.state.doc.toString()).toBe("[](https://www.bilibili.com/video/av1)");
    expect(fetched).toEqual(["https://www.bilibili.com/video/av1"]);

    // 第二步：标题返回后填入
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    expect(view.state.doc.toString()).toBe("[哔哩哔哩](https://www.bilibili.com/video/av1)");
    view.destroy();
  });

  it("粘贴含链接的文本只转换链接部分，保留前后文字", () => {
    const host = mountHost();
    const view = createLiveView(host, {
      doc: "开头",
      fontSize: 14,
      onDocChange: () => {},
      onSave: () => {},
      fetchPageTitle: async () => null,
    });
    view.dispatch({
      changes: { from: 2, insert: " 看 https://example.com/a 详请 " },
      userEvent: "input.paste",
    });
    expect(view.state.doc.toString()).toBe("开头 看 [](https://example.com/a) 详请 ");
    view.destroy();
  });

  it("手动输入链接（非粘贴）不自动转换", () => {
    const host = mountHost();
    const view = createLiveView(host, {
      doc: "",
      fontSize: 14,
      onDocChange: () => {},
      onSave: () => {},
      fetchPageTitle: async () => "不应被调用",
    });
    // 与粘贴相同的文档变化，但没有 input.paste 注解
    view.dispatch({ changes: { from: 0, insert: "https://example.com" } });
    expect(view.state.doc.toString()).toBe("https://example.com");
    view.destroy();
  });

  it("粘贴已经是 [text](url) 的超链接不重复包裹", () => {
    const host = mountHost();
    const view = createLiveView(host, {
      doc: "",
      fontSize: 14,
      onDocChange: () => {},
      onSave: () => {},
      fetchPageTitle: async () => "x",
    });
    view.dispatch({
      changes: { from: 0, insert: "[bilibili](https://www.bilibili.com)" },
      userEvent: "input.paste",
    });
    expect(view.state.doc.toString()).toBe("[bilibili](https://www.bilibili.com)");
    view.destroy();
  });

  it("一次粘贴多个链接：逐个填充标题且位置互不干扰", async () => {
    const host = mountHost();
    const titles: Record<string, string> = {
      "https://a.com": "A站",
      "https://b.com": "B站",
    };
    const view = createLiveView(host, {
      doc: "",
      fontSize: 14,
      onDocChange: () => {},
      onSave: () => {},
      fetchPageTitle: async (url) => titles[url] ?? null,
    });
    view.dispatch({
      changes: { from: 0, insert: "https://a.com 和 https://b.com" },
      userEvent: "input.paste",
    });
    expect(view.state.doc.toString()).toBe("[](https://a.com) 和 [](https://b.com)");
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    expect(view.state.doc.toString()).toBe("[A站](https://a.com) 和 [B站](https://b.com)");
    view.destroy();
  });

  it("setLiveDoc 外部同步保留光标：只有差异段被替换，光标不再瞬移到开头", () => {
    const host = mountHost();
    const view = createLiveView(host, {
      doc: "# 阶段一：这是阶段一",
      fontSize: 14,
      onDocChange: () => {},
      onSave: () => {},
    });
    view.dispatch({ selection: { anchor: 10 } });
    // 模拟防抖回写 echo：尾部少一个字符。旧的全量替换会把光标折叠到 0。
    setLiveDoc(view, "# 阶段一：这是阶段");
    expect(view.state.doc.toString()).toBe("# 阶段一：这是阶段");
    expect(view.state.selection.main.head).toBe(10);
    view.destroy();
  });

  it("setLiveDoc 在输入法组词期间跳过，组词结束后正常同步", () => {
    const host = mountHost();
    const view = createLiveView(host, {
      doc: "biao",
      fontSize: 14,
      onDocChange: () => {},
      onSave: () => {},
    });
    const setComposing = (value: boolean) =>
      Object.defineProperty(view, "composing", { value, writable: true, configurable: true });
    setComposing(true);
    setLiveDoc(view, "biao'ti");
    expect(view.state.doc.toString()).toBe("biao");
    setComposing(false);
    setLiveDoc(view, "biao'ti");
    expect(view.state.doc.toString()).toBe("biao'ti");
    view.destroy();
  });
});
