// todo-reminders 工具单测：mock 掉 Tauri listen，验证过滤与文案逻辑。
import { describe, expect, it, vi } from "vitest";
import type { UnlistenFn } from "@tauri-apps/api/event";

const handlers = new Map<string, (payload: unknown) => void>();
vi.mock("../composables/useTauri", () => ({
  listen: vi.fn(async (event: string, cb: (payload: unknown) => void): Promise<UnlistenFn> => {
    handlers.set(event, cb);
    return () => handlers.delete(event);
  }),
}));

import { reminderToastText, watchTodoReminders, type TodoReminderPayload } from "./todo-reminders";

function fire(payload: unknown) {
  // 与 useTauri.listen 契约一致：handler 直接收 payload（已解包）
  handlers.get("todo://reminder-fired")?.(payload);
}

const base: TodoReminderPayload = {
  id: "t-1",
  sticker_id: 7,
  title: "买菜",
  block_title: "",
  kind: "reminder",
  scheduled_at: "2026-08-25T02:00:00.000Z",
};

describe("watchTodoReminders", () => {
  it("stickerId 过滤：只响应所属便签的提醒", async () => {
    const seen: TodoReminderPayload[] = [];
    await watchTodoReminders({ stickerId: 7, onFire: (p) => seen.push(p) });
    fire({ ...base, sticker_id: 99 }); // 别的便签 → 忽略
    fire(base); // 本便签 → 命中
    expect(seen).toHaveLength(1);
    expect(seen[0].id).toBe("t-1");
  });

  it("缺省 stickerId 时响应全部提醒", async () => {
    const seen: TodoReminderPayload[] = [];
    await watchTodoReminders({ onFire: (p) => seen.push(p) });
    fire({ ...base, id: "t-2", sticker_id: 3 });
    fire(base);
    expect(seen).toHaveLength(2);
  });

  it("忽略非法负载", async () => {
    const seen: TodoReminderPayload[] = [];
    await watchTodoReminders({ onFire: (p) => seen.push(p) });
    fire(null);
    fire(undefined);
    fire({ foo: 1 });
    expect(seen).toHaveLength(0);
  });

  it("取消监听后不再收到事件", async () => {
    const seen: TodoReminderPayload[] = [];
    const unlisten = await watchTodoReminders({ onFire: (p) => seen.push(p) });
    unlisten();
    fire(base);
    expect(seen).toHaveLength(0);
  });
});

describe("reminderToastText", () => {
  it("按类型生成提示文案，空标题回退块标题/未命名", () => {
    expect(reminderToastText(base)).toBe(" 提醒：买菜");
    expect(reminderToastText({ ...base, kind: "due" })).toBe(" 截止：买菜");
    expect(reminderToastText({ ...base, title: "", block_title: "" })).toBe(" 提醒：未命名任务");
  });
});
