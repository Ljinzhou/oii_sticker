import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import TodoWindow from "./TodoWindow.vue";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  onCloseRequested: vi.fn(),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    label: "todo-t-1",
    startDragging: vi.fn(),
    onCloseRequested: mocks.onCloseRequested,
  }),
}));

vi.mock("../../composables/useTauri", () => ({
  invoke: mocks.invoke,
  listen: mocks.listen,
}));

vi.mock("./TodoList.vue", () => ({ default: { template: "<section class='todo-list-stub' />" } }));
vi.mock("./TodoDetail.vue", () => ({ default: { template: "<section class='todo-detail-stub' />" } }));

describe("TodoWindow", () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
    mocks.listen.mockReset();
    mocks.onCloseRequested.mockReset();
    mocks.invoke.mockRejectedValue(new Error("数据库不可用"));
  });

  it("加载失败时显示可见错误，而不是空白窗口", async () => {
    const wrapper = mount(TodoWindow, { global: { plugins: [createPinia()] } });
    await flushPromises();
    expect(wrapper.get("[role='alert']").text()).toContain("数据库不可用");
  });
});
