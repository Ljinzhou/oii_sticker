import { describe, expect, it } from "vitest";
import { mount } from "@vue/test-utils";
import TodoList from "./TodoList.vue";
import type { TodoBlock } from "../../types";

function makeTodo(overrides: Partial<TodoBlock> = {}): TodoBlock {
  return {
    id: "todo-1",
    sticker_id: 1,
    title: "任务",
    block_title: "",
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

  it("拖拽子任务到目标位置时提交该组完整新顺序", async () => {
    const root = makeTodo({ id: "root", title: "根任务" });
    const childOne = makeTodo({ id: "child-1", title: "子任务一", parent_id: root.id });
    const childTwo = makeTodo({ id: "child-2", title: "子任务二", parent_id: root.id });
    const wrapper = mount(TodoList, {
      props: { items: [root, childOne, childTwo], selectedId: null, height: 220 },
    });
    const subTasks = wrapper.findAll(".sub-task");
    await subTasks[0].trigger("dragstart");
    await subTasks[1].trigger("dragenter");
    await subTasks[1].trigger("drop");
    expect(wrapper.emitted("reorder")?.[0]).toEqual([["child-2", "child-1"]]);
  });

  it("拖拽根任务重排提交根组顺序", async () => {
    const rootA = makeTodo({ id: "root-a", title: "任务一" });
    const rootB = makeTodo({ id: "root-b", title: "任务二" });
    const wrapper = mount(TodoList, {
      props: { items: [rootA, rootB], selectedId: null, height: 220 },
    });
    const rows = wrapper.findAll(".todo-list > li").filter((li) => !li.classes().includes("add-child"));
    expect(rows).toHaveLength(2);
    await rows[0].trigger("dragstart");
    await rows[1].trigger("dragenter");
    await rows[1].trigger("drop");
    expect(wrapper.emitted("reorder")?.[0]).toEqual([["root-b", "root-a"]]);
  });

  it("把根任务拖到子任务行时放到其父根之后", async () => {
    const rootA = makeTodo({ id: "root-a", title: "任务一" });
    const child = makeTodo({ id: "child-a", title: "子任务一", parent_id: rootA.id });
    const rootB = makeTodo({ id: "root-b", title: "任务二" });
    const wrapper = mount(TodoList, {
      props: { items: [rootA, child, rootB], selectedId: null, height: 220 },
    });
    const rows = wrapper.findAll(".todo-list > li").filter((li) => !li.classes().includes("add-child"));
    // 拖任务二（最后一个根）到 任务一的子任务行 → 应放到任务一之后（任务一后面原本就是任务二 → 无变化）
    await rows[2].trigger("dragstart");
    await rows[1].trigger("dragenter");
    await rows[1].trigger("drop");
    expect(wrapper.emitted("reorder")).toBeUndefined();

    // 拖任务一到"任务二"的子行？改用 add-child 行：把任务一拖到任务二的 add-child → 任务一放到任务二之后
    const addChildRows = wrapper.findAll(".add-child");
    await rows[0].trigger("dragstart");
    await addChildRows[1].trigger("dragenter");
    await addChildRows[1].trigger("drop");
    expect(wrapper.emitted("reorder")?.[0]).toEqual([["root-b", "root-a"]]);
  });
});
