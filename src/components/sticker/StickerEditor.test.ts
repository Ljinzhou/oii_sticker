import { describe, it, expect, vi, beforeEach } from "vitest";
import { shallowMount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import StickerEditor from "./StickerEditor.vue";
import StickerEditorMarkdown from "./StickerEditorMarkdown.vue";
import StickerEditorLive from "./StickerEditorLive.vue";
import { useSettingsStore } from "../../stores/settings";

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

describe("StickerEditor（容器路由）", () => {
  it("默认 Markdown 模式：渲染 StickerEditorMarkdown（textarea）", () => {
    const wrapper = shallowMount(StickerEditor, {
      props: { content: "# 标题\n- [ ] 任务", stickerId: 7 },
    });
    expect(wrapper.findComponent(StickerEditorMarkdown).exists()).toBe(true);
    expect(wrapper.findComponent(StickerEditorLive).exists()).toBe(false);
  });

  it("editor_mode=live 时渲染 StickerEditorLive（即时预览）", () => {
    const store = useSettingsStore();
    store.config = { entries: { editor_mode: "live" } };
    const wrapper = shallowMount(StickerEditor, {
      props: { content: "# 标题", stickerId: 7 },
    });
    expect(wrapper.findComponent(StickerEditorLive).exists()).toBe(true);
    expect(wrapper.findComponent(StickerEditorMarkdown).exists()).toBe(false);
  });

  it("保存调用 update_sticker_cmd（标题取首行 #）", async () => {
    const wrapper = shallowMount(StickerEditor, {
      props: { content: "# 我的标题\n\n正文", stickerId: 7 },
    });
    const vm = wrapper.vm as unknown as { save: () => Promise<void> };
    await vm.save();
    expect(mocks.invokeMock).toHaveBeenCalledWith("update_sticker_cmd", {
      id: 7,
      patch: { title: "我的标题", content: "# 我的标题\n\n正文" },
    });
  });
});

describe("StickerEditor settings store", () => {
  it("settings.get 读取配置快照", () => {
    const store = useSettingsStore();
    store.config = { entries: { editor_mode: "live", edit_font_size: "20" } };
    expect(store.get("editor_mode", "markdown")).toBe("live");
    expect(store.get("edit_font_size", "14")).toBe("20");
  });
});
