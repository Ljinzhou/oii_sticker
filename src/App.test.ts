import { beforeEach, describe, expect, it, vi } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import { createPinia } from "pinia";

const mocks = vi.hoisted(() => ({
  label: "main",
  invokeMock: vi.fn(async (...args: unknown[]) => {
    const cmd = args[0] as string;
    if (cmd === "workspace_list_cmd") return [];
    if (cmd === "list_stickers_cmd") return [];
    return undefined;
  }),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ label: mocks.label }),
}));

vi.mock("./composables/useTauri", () => ({
  invoke: (...args: unknown[]) => mocks.invokeMock(...args),
}));

vi.mock("./components/console/ConsoleView.vue", () => ({ default: { template: "<div class='console-view' />" } }));
vi.mock("./components/sticker/StickerWindow.vue", () => ({ default: { template: "<div class='sticker-window' />" } }));
vi.mock("./components/todo/TodoWindow.vue", () => ({ default: { template: "<div class='todo-window' />" } }));

describe("App window routing", () => {
  beforeEach(() => {
    mocks.label = "main";
    mocks.invokeMock.mockClear();
    vi.resetModules();
  });

  it("Todo 窗口首次渲染时即挂载 Todo 界面，不经过空白主控台", async () => {
    mocks.label = "todo-t-1";
    const { default: App } = await import("./App.vue");
    const wrapper = mount(App, { global: { plugins: [createPinia()] } });
    await flushPromises();
    expect(wrapper.find(".todo-window").exists()).toBe(true);
    expect(wrapper.find(".console-view").exists()).toBe(false);
  });
});
