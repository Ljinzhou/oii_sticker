import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { useTodoOverviewStore } from "./todo-overview";

const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("../composables/useTauri", () => ({ invoke: invokeMock }));

const item = {
  id: "t-1",
  sticker_id: 7,
  sticker_title: "工作",
  title: "提交周报",
  block_title: "",
  description: null,
  is_completed: false,
  parent_id: null,
  reminder_at: null,
  due_at: null,
  repeat_rule: null,
  created_at: "",
  updated_at: "",
};

beforeEach(() => {
  setActivePinia(createPinia());
  invokeMock.mockReset();
});

describe("todo-overview store", () => {
  it("load 全量拉取（不传 filter）并写入 blocks", async () => {
    invokeMock.mockResolvedValue([item]);
    const store = useTodoOverviewStore();
    await store.load();
    expect(invokeMock).toHaveBeenCalledWith("list_all_todos_cmd", { filter: undefined });
    expect(store.blocks).toEqual([item]);
  });

  it("load 可带筛选条件", async () => {
    invokeMock.mockResolvedValue([]);
    const store = useTodoOverviewStore();
    await store.load({ completed: false, keyword: "周报" });
    expect(invokeMock).toHaveBeenCalledWith("list_all_todos_cmd", {
      filter: { completed: false, keyword: "周报" },
    });
  });

  it("ack 透传命令并回拉刷新", async () => {
    invokeMock.mockResolvedValueOnce({ ...item, reminder_ack_at: "2026-09-09T00:00:00Z" });
    invokeMock.mockResolvedValueOnce([{ ...item, reminder_ack_at: "2026-09-09T00:00:00Z" }]);
    const store = useTodoOverviewStore();
    const block = await store.ack("t-1");
    expect(invokeMock).toHaveBeenNthCalledWith(1, "ack_todo_alert_cmd", { id: "t-1" });
    expect(block?.reminder_ack_at).toBe("2026-09-09T00:00:00Z");
    expect(store.blocks[0].reminder_ack_at).toBe("2026-09-09T00:00:00Z");
  });

  it("toggle / remove / create / patch 透传命令并回拉", async () => {
    const store = useTodoOverviewStore();
    invokeMock.mockResolvedValueOnce({ ...item, is_completed: true });
    invokeMock.mockResolvedValueOnce([{ ...item, is_completed: true }]);
    await store.toggle("t-1", true);
    expect(invokeMock).toHaveBeenNthCalledWith(1, "update_todo_block_cmd", {
      id: "t-1",
      patch: { is_completed: true },
    });
    expect(store.blocks[0].is_completed).toBe(true);

    invokeMock.mockResolvedValueOnce(undefined);
    invokeMock.mockResolvedValueOnce([]);
    await store.remove("t-1");
    expect(invokeMock).toHaveBeenNthCalledWith(3, "delete_todo_block_cmd", { id: "t-1" });

    invokeMock.mockResolvedValueOnce({ ...item, id: "t-2", title: "" });
    invokeMock.mockResolvedValueOnce([{ ...item, id: "t-2", title: "" }]);
    const created = await store.create(7);
    expect(invokeMock).toHaveBeenNthCalledWith(5, "create_todo_block_cmd", { stickerId: 7, parentId: null });
    expect(created.id).toBe("t-2");

    invokeMock.mockResolvedValueOnce({ ...item, due_at: "2026-09-10T10:00:00Z" });
    invokeMock.mockResolvedValueOnce([{ ...item, due_at: "2026-09-10T10:00:00Z" }]);
    await store.patch("t-1", { due_at: "2026-09-10T10:00:00Z" });
    expect(invokeMock).toHaveBeenNthCalledWith(7, "update_todo_block_cmd", {
      id: "t-1",
      patch: { due_at: "2026-09-10T10:00:00Z" },
    });
  });

  it("失败时上抛错误（UI toast 展示）", async () => {
    invokeMock.mockRejectedValue(new Error("最后一个任务不能删除"));
    const store = useTodoOverviewStore();
    await expect(store.remove("t-1")).rejects.toThrow("最后一个任务不能删除");
  });
});