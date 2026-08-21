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

const todo = {
  id: "t-1", sticker_id: 7, title: "购买牛奶", block_title: "",
  description: null, is_completed: false, parent_id: null, reminder_at: null, due_at: null,
  repeat_rule: null, created_at: "", updated_at: "",
};

describe("TodoWindow 完整组件树", () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
    mocks.listen.mockReset();
    mocks.onCloseRequested.mockReset();
    mocks.invoke.mockImplementation((command: string) => {
      if (command === "get_todo_block_cmd") return Promise.resolve(todo);
      if (command === "list_todo_for_sticker_cmd") return Promise.resolve([todo]);
      if (command === "get_config_cmd") return Promise.resolve({ entries: {} });
      return Promise.resolve(undefined);
    });
    mocks.listen.mockResolvedValue(() => {});
    mocks.onCloseRequested.mockResolvedValue(() => {});
  });

  it("加载成功后渲染可操作的 Todo 窗口内容", async () => {
    const wrapper = mount(TodoWindow, { global: { plugins: [createPinia()] } });
    await flushPromises();

    expect(wrapper.get(".todo-window").text()).toContain("购买牛奶");
    expect(wrapper.find(".drag-bar button").exists()).toBe(true);
    expect(wrapper.find(".todo-lower").exists()).toBe(true);
  });

  it("点击列表添加子任务后保持父根选中，可连续添加（不受选中子任务限制）", async () => {
    const child = { ...todo, id: "t-2", title: "", parent_id: "t-1" };
    mocks.invoke.mockImplementation((command: string) => {
      if (command === "get_todo_block_cmd") return Promise.resolve(todo);
      if (command === "list_todo_for_sticker_cmd") return Promise.resolve([todo]);
      if (command === "create_todo_block_cmd") return Promise.resolve(child);
      if (command === "get_config_cmd") return Promise.resolve({ entries: {} });
      return Promise.resolve(undefined);
    });

    const wrapper = mount(TodoWindow, { global: { plugins: [createPinia()] } });
    await flushPromises();
    await wrapper.findAll(".add-child")[0].trigger("click");
    await flushPromises();
    await wrapper.findAll(".add-child")[0].trigger("click");
    await flushPromises();

    const createdCalls = mocks.invoke.mock.calls.filter((call) => call[0] === "create_todo_block_cmd");
    expect(createdCalls).toHaveLength(2);
    expect(createdCalls[0][1]).toEqual({ stickerId: 7, parentId: "t-1" });
    expect(wrapper.findAll(".sub-task")).toHaveLength(1);
    const rootLi = wrapper.findAll(".todo-list > li").find((li) => li.text().includes("购买牛奶"));
    expect(rootLi?.classes()).toContain("selected");
  });
});
