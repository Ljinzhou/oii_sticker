import { describe, it, expect, vi, beforeEach } from "vitest";
import { shallowMount } from "@vue/test-utils";
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
  mocks.invokeMock.mockResolvedValue(undefined);
});

describe("StickerEditor", () => {
  it("编辑工具条不包含关闭按钮（仅保存/取消/设置）", () => {
    const wrapper = shallowMount(StickerEditor, {
      props: { content: "# 标题", stickerId: 7 },
    });
    expect(wrapper.find(".btn.close").exists()).toBe(false);
    expect(wrapper.findAll(".bar .btn").length).toBe(3);
  });
});
