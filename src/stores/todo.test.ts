import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { useTodoStore } from "./todo";

const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("../composables/useTauri", () => ({ invoke: invokeMock }));

const todo = { id: "t-1", sticker_id: 7, title: "任务", block_title: "", description: null, is_completed: false, parent_id: null, reminder_at: null, due_at: null, repeat_rule: null, created_at: "", updated_at: "" };

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
    expect(invokeMock).toHaveBeenNthCalledWith(1, "create_todo_block_cmd", { stickerId: 7, parentId: null });
    expect(invokeMock).toHaveBeenNthCalledWith(2, "update_todo_block_cmd", { id: "t-1", patch: { title: "已改" } });
    expect(store.selected?.title).toBe("已改");
  });

  // 验证：store.create() 在没传 parentId 时显式传 null（与 Rust Option<String> 对齐），
  // 而 create(parentId) 则把字符串原样作为 parentId 传出。
  it("create() 显式传 null 作为 parentId（无父任务语义对齐 Rust Option::None）", async () => {
    const store = useTodoStore(); store.stickerId = 7;
    invokeMock.mockResolvedValueOnce(todo);
    await store.create();
    expect(invokeMock).toHaveBeenCalledWith("create_todo_block_cmd", { stickerId: 7, parentId: null });
  });

  it("create(parentId) 把字符串作为 parentId 传出", async () => {
    const store = useTodoStore(); store.stickerId = 7;
    invokeMock.mockResolvedValueOnce(makeBlock("t-2", "t-1"));
    await store.create("t-1");
    expect(invokeMock).toHaveBeenCalledWith("create_todo_block_cmd", { stickerId: 7, parentId: "t-1" });
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

  it("窗口根任务被删除后刷新列表，而不清空整个便签的任务", async () => {
    const remaining = [makeBlock("t-2"), makeBlock("t-3"), makeBlock("t-4", "t-2")];
    invokeMock.mockResolvedValueOnce(null).mockResolvedValue(remaining);
    const store = useTodoStore();
    store.stickerId = 7;
    store.blocks = [makeBlock("t-1"), ...remaining];
    store.selectedId = "t-1";
    const result = await store.loadForTodo("t-1");
    expect(result).toBeNull();
    expect(invokeMock).toHaveBeenNthCalledWith(2, "list_todo_for_sticker_cmd", { stickerId: 7 });
    expect(store.blocks.map((block) => block.id)).toEqual(["t-2", "t-3", "t-4"]);
    expect(store.selectedId).toBe("t-2");
  });

  it("拖拽重排乐观更新本地顺序，失败时回滚并上抛", async () => {
    invokeMock.mockResolvedValue([makeBlock("t-1"), makeBlock("t-2")]);
    const store = useTodoStore();
    await store.loadForSticker(7);
    // 立即乐观重排（await 之前顺序已变）
    const running = store.reorder(["t-2", "t-1"]);
    expect(store.blocks.map((block) => block.id)).toEqual(["t-2", "t-1"]);
    expect(invokeMock).toHaveBeenCalledWith("reorder_todo_cmd", { ids: ["t-2", "t-1"] });
    await running;
    // 失败：回滚为服务端顺序并上抛错误
    invokeMock.mockRejectedValueOnce(new Error("排序失败")).mockResolvedValue([makeBlock("t-2"), makeBlock("t-1")]);
    await expect(store.reorder(["t-1", "t-2"])).rejects.toThrow("排序失败");
    expect(store.blocks.map((block) => block.id)).toEqual(["t-2", "t-1"]);
  });
});
