import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { useTodoStore } from "./todo";

const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("../composables/useTauri", () => ({ invoke: invokeMock }));

const todo = { id: "t-1", sticker_id: 7, title: "任务", description: null, is_completed: false, parent_id: null, reminder_at: null, due_at: null, repeat_rule: null, created_at: "", updated_at: "" };

function makeBlock(id: string, parentId: string | null = null) {
  return { ...todo, id, parent_id: parentId };
}

beforeEach(() => { setActivePinia(createPinia()); invokeMock.mockReset(); });

describe("todo store", () => {
  it("读取 Todo 后保持指定的选中项", async () => {
    invokeMock.mockResolvedValue([todo]);
    const store = useTodoStore();
    await store.loadForSticker(7, "t-1");
    expect(store.selectedId).toBe("t-1");
    expect(invokeMock).toHaveBeenCalledWith("list_todo_for_sticker_cmd", { stickerId: 7 });
  });

  it("创建和更新使用 camelCase Tauri 参数并更新本地状态", async () => {
    const store = useTodoStore(); store.stickerId = 7;
    invokeMock.mockResolvedValueOnce(todo).mockResolvedValueOnce({ ...todo, title: "已改" });
    await store.create();
    await store.update("t-1", { title: "已改" });
    expect(invokeMock).toHaveBeenNthCalledWith(1, "create_todo_block_cmd", { stickerId: 7, parentId: undefined });
    expect(invokeMock).toHaveBeenNthCalledWith(2, "update_todo_block_cmd", { id: "t-1", patch: { title: "已改" } });
    expect(store.selected?.title).toBe("已改");
  });

  it("删除根任务时级联移除其所有直接子任务", async () => {
    invokeMock.mockResolvedValue([makeBlock("t-1"), makeBlock("t-2", "t-1"), makeBlock("t-3", "t-1"), makeBlock("t-9")]);
    const store = useTodoStore();
    await store.loadForSticker(7);
    await store.remove("t-1");
    expect(invokeMock).toHaveBeenCalledWith("delete_todo_block_cmd", { id: "t-1" });
    expect(store.blocks.map((block) => block.id)).toEqual(["t-9"]);
  });

  it("删除选中的子任务后 selectedId 回退到剩余首块", async () => {
    invokeMock.mockResolvedValue([makeBlock("t-1"), makeBlock("t-2", "t-1")]);
    const store = useTodoStore();
    await store.loadForSticker(7);
    store.selectedId = "t-2";
    await store.remove("t-2");
    expect(store.blocks.map((block) => block.id)).toEqual(["t-1"]);
    expect(store.selectedId).toBe("t-1");
  });
});
