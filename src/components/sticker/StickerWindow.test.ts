import { describe, it, expect, vi, beforeEach } from "vitest";
import { shallowMount, flushPromises } from "@vue/test-utils";
import { createPinia } from "pinia";
import StickerWindow from "./StickerWindow.vue";
import type { StickerMode } from "../../types";

// —— mock Tauri IPC 层：捕获 invoke 调用与 listen 注册 ——
const mocks = vi.hoisted(() => {
  const handlers = new Map<string, (payload: unknown) => void>();
  return {
    handlers,
    invokeMock: vi.fn(),
    listenMock: vi.fn(async (event: string, handler: (p: unknown) => void) => {
      handlers.set(event, handler);
      return () => {};
    }),
  };
});

vi.mock("../../composables/useTauri", () => ({
  invoke: (...args: unknown[]) => mocks.invokeMock(...args),
  listen: (e: string, h: (p: unknown) => void) => mocks.listenMock(e, h),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ label: "sticker-7" }),
}));

const sticker = {
  id: 7,
  parent_id: null,
  title: "便签",
  content: "# 便签",
  heading_level: 0,
  pos_x: 200,
  pos_y: 140,
  width: 400,
  height: 500,
  opacity: 0.9,
  bg_color: null,
  always_on_top: false,
  auto_scroll: false,
  is_completed: false,
  alert_active: false,
  display_mode: "interact",
  created_at: "",
  updated_at: "",
};

const effectivePrefs = {
  opacity: 0.9,
  title_centered: false,
  title_font_size: 14,
  body_font_size: 13,
  bg_color: "#FFF4D6",
  text_color: "#222222",
  auto_scroll_speed: 30,
};

function setupInvoke(mode: StickerMode) {
  mocks.invokeMock.mockImplementation((cmd: string) => {
    switch (cmd) {
      case "get_sticker_cmd":
        return Promise.resolve({ ...sticker, display_mode: mode });
      case "effective_prefs_cmd":
        return Promise.resolve(effectivePrefs);
      default:
        return Promise.resolve(undefined);
    }
  });
}

async function mountSticker(mode: StickerMode) {
  setupInvoke(mode);
  const wrapper = shallowMount(StickerWindow, { global: { plugins: [createPinia()] } });
  await flushPromises();
  return wrapper;
}

beforeEach(() => {
  mocks.handlers.clear();
  mocks.invokeMock.mockReset();
  mocks.listenMock.mockClear();
});

describe("StickerWindow", () => {
  it("交互模式蒙版仅三个功能按钮（无关闭按钮）", async () => {
    const wrapper = await mountSticker("interact");
    const btns = wrapper.findAll(".ov-btn");
    expect(btns.length).toBe(3);
    expect(wrapper.find(".ov-btn.close").exists()).toBe(false);
  });

  it("编辑模式点击关闭按钮调用 hide_sticker_cmd（隐藏而非销毁窗口）", async () => {
    const wrapper = await mountSticker("edit");
    mocks.invokeMock.mockClear();
    wrapper.findComponent({ name: "StickerEditor" }).vm.$emit("closed");
    await flushPromises();
    expect(mocks.invokeMock).toHaveBeenCalledWith("hide_sticker_cmd", { id: 7 });
  });
});
