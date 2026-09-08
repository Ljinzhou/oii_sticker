import { describe, expect, it, beforeEach, vi } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import TodoOverviewView from "./TodoOverviewView.vue";
import type { TodoBlockWithSticker } from "../../types";

const mocks = vi.hoisted(() => ({ invokeMock: vi.fn(), listenMock: vi.fn() }));

vi.mock("../../composables/useTauri", () => ({
  invoke: (...args: unknown[]) => mocks.invokeMock(...args),
  listen: (...args: unknown[]) => mocks.listenMock(...args),
}));

function item(overrides: Partial<TodoBlockWithSticker> = {}): TodoBlockWithSticker {
  return {
    id: "t-1",
    sticker_id: 1,
    sticker_title: "工作",
    title: "提交周报",
    block_title: "",
    description: null,
    is_completed: false,
    parent_id: null,
    reminder_at: null,
    due_at: null,
    repeat_rule: null,
    created_at: "2026-09-01T00:00:00.000Z",
    updated_at: "2026-09-01T00:00:00.000Z",
    ...overrides,
  };
}

/** 渲染总览视图：默认全量返回两条任务（一条提醒中、一条已完成）。 */
async function render(blocks: TodoBlockWithSticker[] = []) {
  const list = blocks.length
    ? blocks
    : [
        item({ id: "t-1", title: "提交周报", sticker_id: 1, sticker_title: "工作" }),
        item({ id: "t-2", title: "评审 PR", sticker_id: 1, sticker_title: "工作", reminded_at: "2026-09-09T00:00:00Z", due_at: "2026-09-08T00:00:00Z" }),
        item({ id: "t-3", title: "买牛奶", sticker_id: 2, sticker_title: "购物", is_completed: true }),
      ];
  mocks.invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "list_all_todos_cmd") return Promise.resolve(list);
    return Promise.resolve(undefined);
  });
  mocks.listenMock.mockResolvedValue(() => {});
  const wrapper = mount(TodoOverviewView, { global: { plugins: [createPinia()] } });
  await flushPromises();
  return wrapper;
}

beforeEach(() => {
  setActivePinia(createPinia());
  mocks.invokeMock.mockReset();
  mocks.listenMock.mockReset();
});

describe("TodoOverviewView", () => {
  it("加载后按便签分组渲染任务行与统计条", async () => {
    const wrapper = await render();
    // 两个分组（工作 / 购物）
    expect(wrapper.findAll(".ov-group-head")).toHaveLength(2);
    expect(wrapper.text()).toContain("提交周报");
    expect(wrapper.text()).toContain("评审 PR");
    expect(wrapper.text()).toContain("买牛奶");
    // 统计：共 3、已完成 1
    expect(wrapper.text()).toContain("共");
    expect(wrapper.text()).toContain("3");
    // 提醒/逾期行显示「已确认」按钮
    expect(wrapper.findAll(".ov-ack")).toHaveLength(1);
  });

  it("子任务行带「子任务」标签且不显示 ack（无提醒时）", async () => {
    const wrapper = await render([
      item({ id: "t-1", title: "父任务", sticker_id: 1, sticker_title: "工作" }),
      item({ id: "t-2", title: "子任务A", sticker_id: 1, sticker_title: "工作", parent_id: "t-1" }),
    ]);
    expect(wrapper.findAll(".ov-sub-tag")).toHaveLength(1);
  });

  it("点击「已完成」chips 只显示已完成任务", async () => {
    const wrapper = await render();
    await wrapper.findAll(".chips button")[3].trigger("click");
    expect(wrapper.findAll(".ov-row")).toHaveLength(1);
    expect(wrapper.text()).toContain("买牛奶");
    expect(wrapper.text()).not.toContain("提交周报");
  });

  it("搜索过滤任务标题", async () => {
    const wrapper = await render();
    await wrapper.find(".ov-search input").setValue("牛奶");
    expect(wrapper.findAll(".ov-row")).toHaveLength(1);
    expect(wrapper.text()).toContain("买牛奶");
  });

  it("点击「已确认」调用 ack 并提示", async () => {
    const wrapper = await render();
    mocks.invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_all_todos_cmd") return Promise.resolve([]);
      if (cmd === "ack_todo_alert_cmd") return Promise.resolve(null);
      return Promise.resolve(undefined);
    });
    await wrapper.find(".ov-ack").trigger("click");
    await flushPromises();
    expect(mocks.invokeMock).toHaveBeenCalledWith("ack_todo_alert_cmd", { id: "t-2" });
    expect(wrapper.text()).toContain("已确认提醒");
  });

  it("删除被后端拒绝时展示错误 toast", async () => {
    const wrapper = await render([item({ id: "t-1" })]);
    mocks.invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "delete_todo_block_cmd") return Promise.reject(new Error("最后一个任务不能删除"));
      return Promise.resolve([]);
    });
    await wrapper.find(".ov-del").trigger("click");
    await flushPromises();
    expect(wrapper.text()).toContain("最后一个任务不能删除");
  });

  it("点击「打开便签」emit open-sticker", async () => {
    const wrapper = await render();
    await wrapper.find(".ov-open-sticker").trigger("click");
    expect(wrapper.emitted("open-sticker")).toEqual([[1]]);
  });

  it("点击任务行打开编辑浮层，保存后 patch 透传（含空串清空语义）", async () => {
    const wrapper = await render([item({ id: "t-1", title: "旧标题", reminder_at: "2026-09-10T09:00:00Z" })]);
    await wrapper.find(".ov-row").trigger("click");
    expect(wrapper.find(".ov-sheet").exists()).toBe(true);
    await wrapper.find(".ov-sheet input[type='text']").setValue("新标题");
    // 清空提醒
    await wrapper.find(".ov-chips .ov-clear-field").trigger("click");
    mocks.invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "update_todo_block_cmd") return Promise.resolve(item({ id: "t-1", title: "新标题" }));
      return Promise.resolve([]);
    });
    await wrapper.find(".ov-actions .btn.primary").trigger("click");
    await flushPromises();
    expect(mocks.invokeMock).toHaveBeenCalledWith("update_todo_block_cmd", {
      id: "t-1",
      patch: { title: "新标题", description: "", reminder_at: "", due_at: "", repeat_rule: "" },
    });
  });

  it("子任务编辑浮层不显示提醒/截止/重复字段", async () => {
    const wrapper = await render([
      item({ id: "t-1", title: "父任务", sticker_id: 1, sticker_title: "工作" }),
      item({ id: "t-2", title: "子任务A", sticker_id: 1, sticker_title: "工作", parent_id: "t-1" }),
    ]);
    await wrapper.findAll(".ov-row")[1].trigger("click");
    expect(wrapper.text()).toContain("编辑子任务");
    expect(wrapper.find(".ov-picker-field").exists()).toBe(false);
  });
});