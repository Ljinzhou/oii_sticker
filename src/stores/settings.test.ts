// settings store：Todo 预设读取（缺省内置/用户 JSON/容错）与保存。
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { useSettingsStore } from "./settings";
import { DEFAULT_PRESETS } from "../utils/presets";

const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("../composables/useTauri", () => ({ invoke: invokeMock }));

beforeEach(() => {
  setActivePinia(createPinia());
  invokeMock.mockReset();
  invokeMock.mockResolvedValue({ entries: {} });
});

describe("settings store todoPresets", () => {
  it("配置缺失时返回出厂内置预设", async () => {
    const store = useSettingsStore();
    await store.refresh();
    expect(store.todoPresets.reminders.map((p) => p.name)).toEqual(DEFAULT_PRESETS.reminders.map((p) => p.name));
    expect(store.todoPresets.due.map((p) => p.name)).toEqual(DEFAULT_PRESETS.due.map((p) => p.name));
    expect(store.todoPresets.repeats.map((p) => p.name)).toEqual(DEFAULT_PRESETS.repeats.map((p) => p.name));
  });

  it("读取用户自定义 JSON（含新增条目）", async () => {
    invokeMock.mockResolvedValue({
      entries: {
        todo_preset_reminders: JSON.stringify([
          { id: "x-1", name: "周五下班前", rule: { kind: "weekday", weekdays: [5], interval: 1, time: "18:00" } },
        ]),
      },
    });
    const store = useSettingsStore();
    await store.refresh();
    expect(store.todoPresets.reminders).toHaveLength(1);
    expect(store.todoPresets.reminders[0].name).toBe("周五下班前");
    // 未配置的组仍用内置
    expect(store.todoPresets.due).toHaveLength(DEFAULT_PRESETS.due.length);
  });

  it("非法 JSON 回退出厂默认", async () => {
    invokeMock.mockResolvedValue({ entries: { todo_preset_due: "not-json{{" } });
    const store = useSettingsStore();
    await store.refresh();
    expect(store.todoPresets.due.map((p) => p.name)).toEqual(DEFAULT_PRESETS.due.map((p) => p.name));
  });

  it("setTodoPresets 写入对应 key 并更新本地快照", async () => {
    const store = useSettingsStore();
    await store.refresh();
    const items = [{ id: "p-9", name: "每天两次", rule: { kind: "cycle", interval: 1, unit: "day", weekdays: null } }];
    await store.setTodoPresets("repeats", items);
    expect(invokeMock).toHaveBeenCalledWith("set_config_cmd", {
      key: "todo_preset_repeats",
      value: JSON.stringify(items),
    });
    expect(store.todoPresets.repeats[0].name).toBe("每天两次");
  });
});