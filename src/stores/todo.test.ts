import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { useTodoStore } from "./todo";

const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("../composables/useTauri", () => ({ invoke: invokeMock }));

const todo = { id: "t-1", sticker_id: 7, title: "任务", description: null, is_completed: false, parent_id: null, reminder_at: null, due_at: null, repeat_rule: null, created_at: "", updated_at: "" };

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
});
