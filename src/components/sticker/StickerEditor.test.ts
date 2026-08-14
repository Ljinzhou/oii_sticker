import { describe, it, expect, vi, beforeEach } from "vitest";
import { shallowMount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import StickerEditor from "./StickerEditor.vue";

// —— mock Tauri IPC 层 ——
const mocks = vi.hoisted(() => ({
  invokeMock: vi.fn(),
}));

vi.mock("../../composables/useTauri", () => ({
  invoke: (...args: unknown[]) => mocks.invokeMock(...args),
}));

beforeEach(() => {
  mocks.invokeMock.mockReset();
  // get_config_cmd 返回配置快照，其余命令返回 undefined
  mocks.invokeMock.mockImplementation((cmd: string) =>
    cmd === "get_config_cmd" ? Promise.resolve({ entries: {} }) : Promise.resolve(undefined),
  );
  setActivePinia(createPinia());
});

describe("StickerEditor", () => {
  it("编辑区为 Markdown 原生文本 textarea（无工具条按钮/无聚焦高亮）", () => {
    const wrapper = shallowMount(StickerEditor, {
      props: { content: "# 标题\n- [ ] 任务", stickerId: 7 },
    });
    // 编辑器内不再渲染保存/取消/关闭按钮（按钮在 StickerWindow overlay 上）
    expect(wrapper.find("button").exists()).toBe(false);
    // 内容为原始 Markdown 文本
    const ta = wrapper.find("textarea");
    expect(ta.exists()).toBe(true);
    expect((ta.element as HTMLTextAreaElement).value).toBe("# 标题\n- [ ] 任务");
    // 默认不显示行号（editor_line_numbers 默认关闭）
    expect(wrapper.find(".gutter").exists()).toBe(false);
  });
});
