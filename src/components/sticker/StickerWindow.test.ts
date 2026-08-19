import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
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

function setupInvoke(mode: StickerMode, autoScroll = false, speed = 30) {
  mocks.invokeMock.mockImplementation((cmd: string) => {
    switch (cmd) {
      case "get_sticker_cmd":
        return Promise.resolve({ ...sticker, auto_scroll: autoScroll, display_mode: mode });
      case "effective_prefs_cmd":
        return Promise.resolve({ ...effectivePrefs, auto_scroll_speed: speed });
      default:
        return Promise.resolve(undefined);
    }
  });
}

async function mountSticker(mode: StickerMode, autoScroll = false, speed = 30) {
  setupInvoke(mode, autoScroll, speed);
  const wrapper = shallowMount(StickerWindow, { global: { plugins: [createPinia()] } });
  await flushPromises();
  return wrapper;
}

beforeEach(() => {
  mocks.handlers.clear();
  mocks.invokeMock.mockReset();
  mocks.listenMock.mockClear();
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("StickerWindow", () => {
  it("交互模式蒙版包含四个功能按钮（含关闭按钮）", async () => {
    const wrapper = await mountSticker("interact");
    const btns = wrapper.findAll(".ov-btn");
    expect(btns.length).toBe(4);
    expect(wrapper.find(".ov-btn.close").exists()).toBe(true);
  });

  it("交互模式点击关闭按钮调用 hide_sticker_cmd（隐藏而非销毁窗口）", async () => {
    const wrapper = await mountSticker("interact");
    mocks.invokeMock.mockClear();
    await wrapper.find(".ov-btn.close").trigger("click");
    await flushPromises();
    expect(mocks.invokeMock).toHaveBeenCalledWith("hide_sticker_cmd", { id: 7 });
  });

  it("编辑模式不显示收起回展示模式按钮", async () => {
    const wrapper = await mountSticker("edit");
    expect(wrapper.find('.ov-btn[title="收起回展示模式"]').exists()).toBe(false);
  });

  it("自动滚动使用便签 effective speed 并在卸载时取消 RAF", async () => {
    const frames: FrameRequestCallback[] = [];
    const cancelFrame = vi.fn();
    vi.stubGlobal("requestAnimationFrame", vi.fn((callback: FrameRequestCallback) => {
      frames.push(callback);
      return frames.length;
    }));
    vi.stubGlobal("cancelAnimationFrame", cancelFrame);

    const wrapper = await mountSticker("display", true, 60);
    const body = wrapper.find<HTMLElement>(".body").element;
    Object.defineProperty(body, "scrollHeight", { configurable: true, value: 500 });
    Object.defineProperty(body, "clientHeight", { configurable: true, value: 100 });

    frames.shift()?.(0);
    frames.shift()?.(1000);
    expect(body.scrollTop).toBeCloseTo(60, 8);

    wrapper.unmount();
    expect(cancelFrame).toHaveBeenCalled();
  });
});
