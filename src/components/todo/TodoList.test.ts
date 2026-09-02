import { describe, expect, it, beforeEach, vi } from "vitest";
import { mount, type DOMWrapper } from "@vue/test-utils";
import TodoList from "./TodoList.vue";
import type { TodoBlock } from "../../types";

/** 当前块的 id（三层结构第 0 层；列表里不显示块本身）。 */
const BLOCK = "block-1";

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

/** 父任务：挂在块下（第 1 层）。 */
function parentTask(id: string, title: string) {
  return makeTodo({ id, title, parent_id: BLOCK });
}
/** 子任务：挂在父任务下（第 2 层）。 */
function subTask(id: string, title: string, parentId: string) {
  return makeTodo({ id, title, parent_id: parentId });
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

describe("TodoList 三层结构（父任务 / 子任务）", () => {
  it("渲染父任务及其子任务，父任务下带「添加子任务」", async () => {
    const p = parentTask("p1", "父任务一");
    const s1 = subTask("s1", "子任务一", p.id);
    const s2 = subTask("s2", "子任务二", p.id);
    const wrapper = mount(TodoList, {
      props: { items: [p, s1, s2], blockId: BLOCK, selectedId: null, height: 220 },
    });

    expect(wrapper.findAll(".sub-task")).toHaveLength(2);
    expect(wrapper.text()).toContain("子任务一");
    expect(wrapper.text()).toContain("子任务二");
    // 只有父任务下才有「添加子任务」
    expect(wrapper.findAll(".add-child")).toHaveLength(1);

    await wrapper.get(".add-child").trigger("click");
    expect(wrapper.emitted("createChild")).toEqual([[p.id]]);
  });

  it("子任务行不再渲染「添加子任务」（子任务下不能继续挂）", () => {
    const p = parentTask("p1", "父任务一");
    const s1 = subTask("s1", "子任务一", p.id);
    const s2 = subTask("s2", "子任务二", p.id);
    const wrapper = mount(TodoList, {
      props: { items: [p, s1, s2], blockId: BLOCK, selectedId: null, height: 220 },
    });
    // 两个子任务都渲染了，但 add-child 仍只有父任务那一条
    expect(wrapper.findAll(".sub-task")).toHaveLength(2);
    expect(wrapper.findAll(".add-child")).toHaveLength(1);
  });

  it("每行渲染删除按钮，点击 emit remove 且不触发 select", async () => {
    const p = parentTask("p1", "父任务一");
    const s1 = subTask("s1", "子任务一", p.id);
    const wrapper = mount(TodoList, {
      props: { items: [p, s1], blockId: BLOCK, selectedId: p.id, height: 220 },
    });

    const deletes = wrapper.findAll(".row-delete");
    expect(deletes).toHaveLength(2);

    await deletes[1].trigger("click");
    expect(wrapper.emitted("remove")).toEqual([[s1.id]]);
    expect(wrapper.emitted("select")).toBeUndefined();
  });

  it("拖拽子任务到目标位置时提交该组完整新顺序", async () => {
    const p = parentTask("p1", "父任务一");
    const s1 = subTask("s1", "子任务一", p.id);
    const s2 = subTask("s2", "子任务二", p.id);
    const wrapper = mount(TodoList, {
      props: { items: [p, s1, s2], blockId: BLOCK, selectedId: null, height: 220 },
    });
    const subTasks = wrapper.findAll(".sub-task");
    await dragRow(subTasks[0], subTasks[1]);
    expect(wrapper.emitted("reorder")?.[0]).toEqual([["s2", "s1"]]);
  });

  it("拖拽父任务重排提交父任务组顺序", async () => {
    const pA = parentTask("p-a", "任务一");
    const pB = parentTask("p-b", "任务二");
    const wrapper = mount(TodoList, {
      props: { items: [pA, pB], blockId: BLOCK, selectedId: null, height: 220 },
    });
    const rows = wrapper.findAll(".todo-list > li").filter((li) => !li.classes().includes("add-child"));
    expect(rows).toHaveLength(2);
    await dragRow(rows[0], rows[1]);
    expect(wrapper.emitted("reorder")?.[0]).toEqual([["p-b", "p-a"]]);
  });

  it("把父任务拖到子任务行时放到其父之后；拖到 add-child 行放到该父之后", async () => {
    const pA = parentTask("p-a", "任务一");
    const sA = subTask("s-a", "子任务一", pA.id);
    const pB = parentTask("p-b", "任务二");
    const wrapper = mount(TodoList, {
      props: { items: [pA, sA, pB], blockId: BLOCK, selectedId: null, height: 220 },
    });
    const rows = wrapper.findAll(".todo-list > li").filter((li) => !li.classes().includes("add-child"));
    // 拖任务二到 任务一的子任务行 → 应放到任务一之后（原本就是该顺序 → 无变化）
    await dragRow(rows[2], rows[1]);
    expect(wrapper.emitted("reorder")).toBeUndefined();

    const addChildRows = wrapper.findAll(".add-child");
    await dragRow(rows[0], addChildRows[1]);
    expect(wrapper.emitted("reorder")?.[0]).toEqual([["p-b", "p-a"]]);
  });

  it("未移动的按下-松开视为点击选择行", async () => {
    const pA = parentTask("p-a", "任务一");
    const wrapper = mount(TodoList, {
      props: { items: [pA], blockId: BLOCK, selectedId: null, height: 220 },
    });
    const row = wrapper.get(".todo-list > li");
    await row.trigger("click");
    expect(wrapper.emitted("select")?.[0]).toEqual([pA.id]);
  });

  it("不属于当前块的任务不渲染（blockId 过滤）", () => {
    const mine = parentTask("p1", "本块任务");
    const other = makeTodo({ id: "other", title: "别块任务", parent_id: "block-2" });
    const wrapper = mount(TodoList, {
      props: { items: [mine, other], blockId: BLOCK, selectedId: null, height: 220 },
    });
    expect(wrapper.text()).toContain("本块任务");
    expect(wrapper.text()).not.toContain("别块任务");
  });
});
