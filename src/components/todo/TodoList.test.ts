import { describe, expect, it } from "vitest";
import { mount } from "@vue/test-utils";
import TodoList from "./TodoList.vue";
import type { TodoBlock } from "../../types";

function makeTodo(overrides: Partial<TodoBlock> = {}): TodoBlock {
  return {
    id: "todo-1",
    sticker_id: 1,
    title: "任务",
    description: null,
    is_completed: false,
    parent_id: null,
    reminder_at: null,
    due_at: null,
    repeat_rule: null,
    created_at: "2026-08-20T00:00:00.000Z",
    updated_at: "2026-08-20T00:00:00.000Z",
    ...overrides,
  };
}

describe("TodoList child tasks", () => {
  it("renders every direct child and keeps the root add-child action", async () => {
    const root = makeTodo({ id: "root", title: "根任务" });
    const childOne = makeTodo({ id: "child-1", title: "子任务一", parent_id: root.id });
    const childTwo = makeTodo({ id: "child-2", title: "子任务二", parent_id: root.id });
    const wrapper = mount(TodoList, {
      props: { items: [root, childOne, childTwo], selectedId: null, height: 220 },
    });

    expect(wrapper.findAll(".sub-task")).toHaveLength(2);
    expect(wrapper.text()).toContain("子任务一");
    expect(wrapper.text()).toContain("子任务二");
    expect(wrapper.findAll(".add-child")).toHaveLength(1);

    await wrapper.get(".add-child").trigger("click");
    expect(wrapper.emitted("createChild")).toEqual([[root.id]]);
  });

  it("每行渲染删除按钮，点击 emit remove 且不触发 select", async () => {
    const root = makeTodo({ id: "root", title: "根任务" });
    const child1 = makeTodo({ id: "child-1", title: "子任务一", parent_id: root.id });
    const wrapper = mount(TodoList, { props: { items: [root, child1], selectedId: root.id, height: 220 } });

    const deletes = wrapper.findAll(".row-delete");
    expect(deletes).toHaveLength(2);

    await deletes[1].trigger("click");
    expect(wrapper.emitted("remove")).toEqual([[child1.id]]);
    expect(wrapper.emitted("select")).toBeUndefined();
  });
});
