import { beforeEach, describe, expect, it, vi } from "vitest";
import { mount } from "@vue/test-utils";
import { createPinia } from "pinia";

const mocks = vi.hoisted(() => ({ label: "main" }));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ label: mocks.label }),
}));

vi.mock("./components/console/ConsoleView.vue", () => ({ default: { template: "<div class='console-view' />" } }));
vi.mock("./components/sticker/StickerWindow.vue", () => ({ default: { template: "<div class='sticker-window' />" } }));
vi.mock("./components/todo/TodoWindow.vue", () => ({ default: { template: "<div class='todo-window' />" } }));

describe("App window routing", () => {
  beforeEach(() => {
    mocks.label = "main";
    vi.resetModules();
  });

  it("Todo 窗口首次渲染时即挂载 Todo 界面，不经过空白主控台", async () => {
    mocks.label = "todo-t-1";
    const { default: App } = await import("./App.vue");
    const wrapper = mount(App, { global: { plugins: [createPinia()] } });
    expect(wrapper.find(".todo-window").exists()).toBe(true);
    expect(wrapper.find(".console-view").exists()).toBe(false);
  });
});
