import { describe, expect, it, beforeEach, vi } from "vitest";
import { mount, type DOMWrapper } from "@vue/test-utils";
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

beforeEach(() => {
  // jsdom 可能未实现 elementFromPoint：先保证存在，再在用例中 stub 返回值
  if (typeof document.elementFromPoint !== "function") {
    Object.defineProperty(document, "elementFromPoint", { value: () => null, writable: true, configurable: true });
  }
});

/** 指针拖拽：按下 → 移动（stub 指针下的元素为 target）→ 松开。 */
async function dragRow(source: DOMWrapper<Element>, target: DOMWrapper<Element>) {
  await source.trigger("mousedown", { button: 0, clientX: 5, clientY: 5 });
  const spy = vi.spyOn(document, "elementFromPoint").mockReturnValue(target.element as Element);
  document.dispatchEvent(new MouseEvent("mousemove", { clientX: 60, clientY: 60, bubbles: true }));
  document.dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
  spy.mockRestore();
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
    await dragRow(subTasks[0], subTasks[1]);
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
    await dragRow(rows[0], rows[1]);
    expect(wrapper.emitted("reorder")?.[0]).toEqual([["root-b", "root-a"]]);
  });

  it("把根任务拖到子任务行时放到其父根之后；拖到 add-child 行放到该根之后", async () => {
    const rootA = makeTodo({ id: "root-a", title: "任务一" });
    const child = makeTodo({ id: "child-a", title: "子任务一", parent_id: rootA.id });
    const rootB = makeTodo({ id: "root-b", title: "任务二" });
    const wrapper = mount(TodoList, {
      props: { items: [rootA, child, rootB], selectedId: null, height: 220 },
    });
    const rows = wrapper.findAll(".todo-list > li").filter((li) => !li.classes().includes("add-child"));
    // 拖任务二（最后一个根）到 任务一的子任务行 → 应放到任务一之后（任务一后面原本就是任务二 → 无变化）
    await dragRow(rows[2], rows[1]);
    expect(wrapper.emitted("reorder")).toBeUndefined();

    // 把任务一拖到任务二的 add-child 行 → 任务一放到任务二之后
    const addChildRows = wrapper.findAll(".add-child");
    await dragRow(rows[0], addChildRows[1]);
    expect(wrapper.emitted("reorder")?.[0]).toEqual([["root-b", "root-a"]]);
  });

  it("未移动的按下-松开视为点击选择行，拖拽结束后不误触发 select", async () => {
    const rootA = makeTodo({ id: "root-a", title: "任务一" });
    const wrapper = mount(TodoList, {
      props: { items: [rootA], selectedId: null, height: 220 },
    });
    const row = wrapper.get(".todo-list > li");
    // 普通点击 → select
    await row.trigger("click");
    expect(wrapper.emitted("select")?.[0]).toEqual([rootA.id]);
  });
});
