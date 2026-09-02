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

// 三层结构：块(第 0 层) → 父任务(第 1 层) → 子任务(第 2 层)
// 窗口 label 是 todo-t-1，即 todoId = 块 id。
const block = {
  id: "t-1", sticker_id: 7, title: "", block_title: "购物清单",
  description: null, is_completed: false, parent_id: null, reminder_at: null, due_at: null,
  repeat_rule: null, created_at: "", updated_at: "",
};
/** 父任务：挂在块 t-1 下。 */
const task = { ...block, id: "t-2", title: "购买牛奶", block_title: "", parent_id: "t-1" };

describe("TodoWindow 完整组件树", () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
    mocks.listen.mockReset();
    mocks.onCloseRequested.mockReset();
    mocks.invoke.mockImplementation((command: string) => {
      if (command === "get_todo_block_cmd") return Promise.resolve(block);
      if (command === "list_todo_for_sticker_cmd") return Promise.resolve([block, task]);
      if (command === "get_config_cmd") return Promise.resolve({ entries: {} });
      return Promise.resolve(undefined);
    });
    mocks.listen.mockResolvedValue(() => {});
    mocks.onCloseRequested.mockResolvedValue(() => {});
  });

  it("加载成功后渲染可操作的 Todo 窗口内容", async () => {
    const wrapper = mount(TodoWindow, { global: { plugins: [createPinia()] } });
    await flushPromises();

    // 块名显示在顶部输入框，父任务名显示在列表
    expect(wrapper.get(".todo-window").text()).toContain("购买牛奶");
    expect(wrapper.find(".drag-bar button").exists()).toBe(true);
    expect(wrapper.find(".todo-lower").exists()).toBe(true);
  });

  it("点击列表添加子任务后保持父任务选中，可连续添加", async () => {
    const sub = { ...task, id: "t-3", title: "", parent_id: "t-2" };
    mocks.invoke.mockImplementation((command: string) => {
      if (command === "get_todo_block_cmd") return Promise.resolve(block);
      if (command === "list_todo_for_sticker_cmd") return Promise.resolve([block, task]);
      if (command === "create_todo_block_cmd") return Promise.resolve(sub);
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
    // 子任务挂在父任务 t-2 下（不是块 t-1）
    expect(createdCalls[0][1]).toEqual({ stickerId: 7, parentId: "t-2" });
    expect(wrapper.findAll(".sub-task")).toHaveLength(1);
    const parentLi = wrapper.findAll(".todo-list > li").find((li) => li.text().includes("购买牛奶"));
    expect(parentLi?.classes()).toContain("selected");
  });

  // 核心修复：块只能由编辑器 / 菜单创建。
  // 「+ 新建任务」永远在当前块下建**父任务**，绝不新建块、绝不补 markdown 标签，
  // 因此不会出现"点 3 次冒出 3 张卡片"的问题。
  it("「+ 新建任务」在当前块下建父任务，不建块也不补标签", async () => {
    const fresh = { ...task, id: "t-9", title: "新任务" };
    mocks.invoke.mockImplementation((command: string) => {
      if (command === "get_todo_block_cmd") return Promise.resolve(block);
      if (command === "list_todo_for_sticker_cmd") return Promise.resolve([block, task]);
      if (command === "create_todo_block_cmd") return Promise.resolve(fresh);
      if (command === "get_config_cmd") return Promise.resolve({ entries: {} });
      return Promise.resolve(undefined);
    });

    const wrapper = mount(TodoWindow, { global: { plugins: [createPinia()] } });
    await flushPromises();

    await wrapper.find("button.add-task").trigger("click");
    await flushPromises();

    const createdCalls = mocks.invoke.mock.calls.filter((call) => call[0] === "create_todo_block_cmd");
    expect(createdCalls).toHaveLength(1);
    // 关键：parentId = 当前块 id → 建的是父任务，不是新块
    expect(createdCalls[0][1]).toEqual({ stickerId: 7, parentId: "t-1" });
    // 关键：没有调 sync_todo_marker_cmd —— 没有新块，不需要补标签
    const syncCalls = mocks.invoke.mock.calls.filter((call) => call[0] === "sync_todo_marker_cmd");
    expect(syncCalls).toHaveLength(0);
  });

  it("Todo 窗口不再提供「新建块」入口（块只能由 / 菜单创建）", async () => {
    const wrapper = mount(TodoWindow, { global: { plugins: [createPinia()] } });
    await flushPromises();
    expect(wrapper.find("button.add-block").exists()).toBe(false);
    // 只有「新建任务」一个按钮
    expect(wrapper.findAll(".header-actions button")).toHaveLength(1);
  });

  it("子任务行下不出现「添加子任务」（子任务不能再挂子任务）", async () => {
    const sub = { ...task, id: "t-3", title: "子任务", parent_id: "t-2" };
    mocks.invoke.mockImplementation((command: string) => {
      if (command === "get_todo_block_cmd") return Promise.resolve(block);
      if (command === "list_todo_for_sticker_cmd") return Promise.resolve([block, task, sub]);
      if (command === "get_config_cmd") return Promise.resolve({ entries: {} });
      return Promise.resolve(undefined);
    });

    const wrapper = mount(TodoWindow, { global: { plugins: [createPinia()] } });
    await flushPromises();

    // 1 个父任务 → 只有 1 条「添加子任务」，子任务自身不产生
    expect(wrapper.findAll(".sub-task")).toHaveLength(1);
    expect(wrapper.findAll(".add-child")).toHaveLength(1);
  });
});
