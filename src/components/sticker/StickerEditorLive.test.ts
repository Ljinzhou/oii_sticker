import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import StickerEditorLive from "./StickerEditorLive.vue";

// ── CM6 内核假实现：记录 createLiveView 收到的 opts，编辑器文档可编程 ──
const mocks = vi.hoisted(() => {
  type LiveOpts = {
    doc: string;
    onDocChange: (doc: string) => void;
    onSave?: () => void;
  };
  return {
    createdOpts: [] as LiveOpts[],
    editorDoc: { value: "" },
    setLiveDoc: vi.fn(),
    setLiveFontFamily: vi.fn(),
    setLiveFontSize: vi.fn(),
    setLiveLineNumbers: vi.fn(),
    setLiveTodoBlocksInView: vi.fn(),
  };
});

vi.mock("./live/LiveEditorView", () => ({
  createLiveView: vi.fn((_parent: unknown, opts: (typeof mocks.createdOpts)[number]) => {
    mocks.createdOpts.push(opts);
    mocks.editorDoc.value = opts.doc;
    return {
      get state() {
        return { doc: { toString: () => mocks.editorDoc.value } };
      },
      composing: false,
      focus() {},
      destroy() {},
    };
  }),
  setLiveDoc: mocks.setLiveDoc,
  setLiveFontFamily: mocks.setLiveFontFamily,
  setLiveFontSize: mocks.setLiveFontSize,
  setLiveLineNumbers: mocks.setLiveLineNumbers,
  setLiveTodoBlocksInView: mocks.setLiveTodoBlocksInView,
}));

function lastOpts() {
  return mocks.createdOpts[mocks.createdOpts.length - 1];
}

/** 模拟用户在编辑器里输入：内核文档前进并触发 onDocChange。 */
function typeDoc(doc: string) {
  mocks.editorDoc.value = doc;
  lastOpts().onDocChange(doc);
}

function mountEditor(modelValue = "") {
  return mount(StickerEditorLive, {
    props: {
      modelValue,
      fontSize: 14,
      fontFamily: "Microsoft YaHei",
      showLineNumbers: false,
      todoBlocks: [],
      slashOpen: false,
    },
  });
}

describe("StickerEditorLive（防抖回写 / echo 抑制回归）", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    mocks.createdOpts.length = 0;
    mocks.editorDoc.value = "";
    mocks.setLiveDoc.mockClear();
  });

  afterEach(() => {
    vi.useRealTimers();
    document.body.innerHTML = "";
  });

  it("输入又删回原值：陈旧防抖定时器不再把已删字符写回（退格删不干净回归）", async () => {
    const wrapper = mountEditor("biao't");
    await flushPromises();
    // 输入 i → 调度 400ms 后回写 "biao'ti"
    typeDoc("biao'ti");
    // 快速退格删回原值 "biao't"（与 modelValue 相同）。
    // 修复前：scheduleEmit 提前 return 留下旧定时器，稍后仍 emit "biao'ti"，
    // 父级 v-model 把它同步回编辑器 → 刚删的字符复活、光标被甩到开头。
    typeDoc("biao't");
    await vi.advanceTimersByTimeAsync(500);
    expect(wrapper.emitted("update:modelValue")).toBeUndefined();
    expect(mocks.setLiveDoc).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("停顿超过防抖窗口：仍按最新内容正常回写一次", async () => {
    const wrapper = mountEditor("abc");
    await flushPromises();
    typeDoc("abcd");
    await vi.advanceTimersByTimeAsync(400);
    const emissions = wrapper.emitted("update:modelValue") ?? [];
    expect(emissions[emissions.length - 1]).toEqual(["abcd"]);
    wrapper.unmount();
  });

  it("自己的回写被父级原样传回（echo）时不反向覆盖编辑器", async () => {
    const wrapper = mountEditor("abc");
    await flushPromises();
    typeDoc("abcd");
    await vi.advanceTimersByTimeAsync(400);
    const emissions = wrapper.emitted("update:modelValue") ?? [];
    expect(emissions[emissions.length - 1]).toEqual(["abcd"]);
    // 父级 v-model 把刚回写的值原样传回来
    await wrapper.setProps({ modelValue: "abcd" });
    // echo 不是外部更新：绝不能 setLiveDoc 反向同步（否则打断连续输入）
    expect(mocks.setLiveDoc).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("echo 之后真正的外部更新（保存后 load / push-update）照常同步进编辑器", async () => {
    const wrapper = mountEditor("abc");
    await flushPromises();
    typeDoc("abcd");
    await vi.advanceTimersByTimeAsync(400);
    await wrapper.setProps({ modelValue: "abcd" }); // echo，应被跳过
    await wrapper.setProps({ modelValue: "外部内容" }); // 外部更新，应生效
    expect(mocks.setLiveDoc).toHaveBeenCalledTimes(1);
    expect(mocks.setLiveDoc).toHaveBeenCalledWith(expect.anything(), "外部内容");
    wrapper.unmount();
  });

  it("flush() 立即回写且同样抑制自身 echo", async () => {
    const wrapper = mountEditor("");
    await flushPromises();
    typeDoc("draft");
    const vm = wrapper.vm as unknown as { flush: () => void };
    vm.flush();
    const emissions = wrapper.emitted("update:modelValue") ?? [];
    expect(emissions[emissions.length - 1]).toEqual(["draft"]);
    await wrapper.setProps({ modelValue: "draft" });
    expect(mocks.setLiveDoc).not.toHaveBeenCalled();
    wrapper.unmount();
  });
});
