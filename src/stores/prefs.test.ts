import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { usePrefsStore } from "./prefs";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("../composables/useTauri", () => ({
  invoke: invokeMock,
}));

beforeEach(() => {
  setActivePinia(createPinia());
  invokeMock.mockReset();
});

describe("prefs store", () => {
  it("本地更新便签自动滚动速度", () => {
    const store = usePrefsStore();
    store.effective = {
      opacity: 0.9,
      title_centered: false,
      title_font_size: 14,
      body_font_size: 13,
      bg_color: "#FFF4D6",
      text_color: "#222222",
      auto_scroll_speed: 30,
    };

    store.applyLocal({ auto_scroll_speed: 65 });

    expect(store.effective.auto_scroll_speed).toBe(65);
  });

  it("保存自动滚动速度时传递便签偏好字段", async () => {
    const store = usePrefsStore();
    invokeMock
      .mockResolvedValueOnce(undefined)
      .mockResolvedValueOnce({ auto_scroll_speed: 65 });

    await store.save(7, { auto_scroll_speed: 65 });

    expect(invokeMock).toHaveBeenNthCalledWith(1, "update_sticker_prefs_cmd", {
      prefs: { sticker_id: 7, auto_scroll_speed: 65 },
    });
  });
});
