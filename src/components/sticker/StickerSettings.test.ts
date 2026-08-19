import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import StickerSettings from "./StickerSettings.vue";

const invokeMock = vi.hoisted(() => vi.fn());
const effectiveSpeed = vi.hoisted(() => ({ value: 45 }));

vi.mock("../../composables/useTauri", () => ({
  invoke: invokeMock,
}));

beforeEach(() => {
  invokeMock.mockImplementation((command: string) => {
    if (command === "effective_prefs_cmd") {
      return Promise.resolve({
        opacity: 0.9,
        title_centered: false,
        title_font_size: 14,
        body_font_size: 13,
        bg_color: "#FFF4D6",
        text_color: "#222222",
        auto_scroll_speed: effectiveSpeed.value,
      });
    }
    if (command === "get_sticker_cmd") {
      return Promise.resolve({ always_on_top: false, auto_scroll: true });
    }
    if (command === "get_reminder_cmd") return Promise.resolve(null);
    if (command === "update_sticker_prefs_cmd") return Promise.resolve(undefined);
    return Promise.resolve(undefined);
  });
});

describe("StickerSettings 自动滚动速度", () => {
  beforeEach(() => {
    effectiveSpeed.value = 45;
  });

  it("读取 effective 初值并提供 5-120、步进 5 的控件", async () => {
    const wrapper = mount(StickerSettings, {
      props: { stickerId: 7 },
      global: { plugins: [createPinia()] },
    });
    await flushPromises();

    const speed = wrapper.get('[data-testid="auto-scroll-speed"]');
    expect(speed.attributes("type")).toBe("range");
    expect(speed.attributes("min")).toBe("5");
    expect(speed.attributes("max")).toBe("120");
    expect(speed.attributes("step")).toBe("5");
    expect((speed.element as HTMLInputElement).value).toBe("45");
  });

  it("调整速度后即时更新 effective 并持久化字段", async () => {
    const wrapper = mount(StickerSettings, {
      props: { stickerId: 7 },
      global: { plugins: [createPinia()] },
    });
    await flushPromises();

    const speed = wrapper.get<HTMLInputElement>('[data-testid="auto-scroll-speed"]');
    speed.element.value = "70";
    await speed.trigger("input");
    await speed.trigger("change");
    await new Promise((resolve) => window.setTimeout(resolve, 300));
    await flushPromises();

    expect(invokeMock).toHaveBeenCalledWith("update_sticker_prefs_cmd", {
      prefs: expect.objectContaining({ sticker_id: 7, auto_scroll_speed: 70 }),
    });
  });

  it("读取历史非步进速度时归一化到最近的 5 的倍数", async () => {
    effectiveSpeed.value = 32;
    const wrapper = mount(StickerSettings, {
      props: { stickerId: 7 },
      global: { plugins: [createPinia()] },
    });
    await flushPromises();

    expect(wrapper.get<HTMLInputElement>('[data-testid="auto-scroll-speed"]').element.value).toBe("30");
  });
});
