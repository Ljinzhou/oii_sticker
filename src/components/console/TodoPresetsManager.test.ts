// TodoPresetsManager：预设列表渲染、添加/编辑/删除、保存落库。
import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import TodoPresetsManager from "./TodoPresetsManager.vue";
import { DEFAULT_PRESETS } from "../../utils/presets";

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("../../composables/useTauri", () => ({ invoke: mocks.invoke, listen: vi.fn() }));

async function mountManager(kind: "reminders" | "due" | "repeats" = "reminders") {
  const wrapper = mount(TodoPresetsManager, {
    props: { kind, title: "提醒时间预设", hint: "test" },
    global: { plugins: [createPinia()] },
  });
  await flushPromises();
  return wrapper;
}

function setConfig(entries: Record<string, string>) {
  mocks.invoke.mockImplementation((command: string) => {
    if (command === "get_config_cmd") return Promise.resolve({ entries });
    if (command === "set_config_cmd") return Promise.resolve(undefined);
    return Promise.resolve(undefined);
  });
}

beforeEach(() => {
  mocks.invoke.mockReset();
  setConfig({});
});

describe("TodoPresetsManager 列表", () => {
  it("无配置时渲染出厂内置预设（名称 + 描述 + 徽标 + 操作按钮）", async () => {
    const wrapper = await mountManager("reminders");
    const names = wrapper.findAll(".preset-name").map((el) => el.text());
    expect(names).toEqual(DEFAULT_PRESETS.reminders.map((p) => p.name));
    expect(wrapper.findAll(".preset-rule")[0].text()).toContain("小时");
    expect(wrapper.findAll(".preset-badge")[0].text()).toBe("相对");
    expect(wrapper.findAll(".icon-btn").length).toBe(DEFAULT_PRESETS.reminders.length * 2);
  });

  it("读取用户配置的预设（新增条目）", async () => {
    setConfig({
      todo_preset_reminders: JSON.stringify([
        { id: "x-1", name: "周五下班前", rule: { kind: "weekday", weekdays: [5], interval: 1, time: "18:00" } },
      ]),
    });
    const wrapper = await mountManager("reminders");
    expect(wrapper.find(".preset-name").text()).toBe("周五下班前");
    expect(wrapper.findAll(".preset-name")).toHaveLength(1);
  });

  it("删除预设：调用 set_config_cmd 并更新列表", async () => {
    const wrapper = await mountManager("reminders");
    await wrapper.findAll(".icon-btn.del")[0].trigger("click");
    await flushPromises();
    expect(mocks.invoke).toHaveBeenCalledWith("set_config_cmd", {
      key: "todo_preset_reminders",
      value: JSON.stringify(DEFAULT_PRESETS.reminders.slice(1)),
    });
    expect(wrapper.findAll(".preset-name")).toHaveLength(DEFAULT_PRESETS.reminders.length - 1);
  });
});

describe("TodoPresetsManager 添加/编辑", () => {
  it("添加日历预设：填名称 → 保存 → 追加到列表并落库", async () => {
    const wrapper = await mountManager("reminders");
    await wrapper.find(".add-btn").trigger("click");
    expect(wrapper.find(".overlay").exists()).toBe(true);

    await wrapper.find('input[type="text"]').setValue("圣诞提醒");
    const tabButtons = wrapper.findAll(".rule-tabs button");
    await tabButtons[1].trigger("click"); // 日历时间
    await wrapper.find('input[type="date"]').setValue("2026-12-25");
    await wrapper.find('input[type="time"]').setValue("08:30");
    await wrapper.find(".btn-save").trigger("click");
    await flushPromises();

    const saved = mocks.invoke.mock.calls.find((c) => c[0] === "set_config_cmd")!;
    expect(saved).toBeTruthy();
    const items = JSON.parse(saved[1].value);
    expect(items).toHaveLength(DEFAULT_PRESETS.reminders.length + 1);
    expect(items[items.length - 1].name).toBe("圣诞提醒");
    expect(items[items.length - 1].rule).toEqual({ kind: "calendar", date: "2026-12-25", time: "08:30" });
    expect(wrapper.findAll(".preset-name").at(-1)?.text()).toBe("圣诞提醒");
  });

  it("重复预设只提供「周期」tab，保存输出 cycle 规则到 repeats key", async () => {
    const wrapper = await mountManager("repeats");
    await wrapper.find(".add-btn").trigger("click");
    expect(wrapper.findAll(".rule-tabs button").map((b) => b.text())).toEqual(["重复周期"]);
    await wrapper.find('input[type="text"]').setValue("工作日");
    // 周期：每 1 周 + 勾选周一至周五（0=周日，1-5=周一~周五）
    const unitSelect = wrapper.find(".rule-pane select");
    await unitSelect.setValue("week");
    const grid = wrapper.findAll(".weekday-grid")[0];
    const dayButtons = grid.findAll("button");
    for (const day of [1, 2, 3, 4, 5]) dayButtons[day].trigger("click"); // 周一至周五
    await wrapper.find(".btn-save").trigger("click");
    await flushPromises();

    const saved = mocks.invoke.mock.calls.find((c) => c[0] === "set_config_cmd")!;
    expect(saved[1].key).toBe("todo_preset_repeats");
    const items = JSON.parse(saved[1].value);
    expect(items[items.length - 1].rule).toEqual({ kind: "cycle", interval: 1, unit: "week", weekdays: [1, 2, 3, 4, 5] });
  });

  it("编辑预设：模态预填并可改名称保存", async () => {
    const wrapper = await mountManager("reminders");
    await wrapper.findAll(".icon-btn")[0].trigger("click"); // 编辑第一个
    const nameInput = wrapper.find('input[type="text"]');
    expect((nameInput.element as HTMLInputElement).value).toBe(DEFAULT_PRESETS.reminders[0].name);
    await nameInput.setValue("改后名称");
    await wrapper.find(".btn-save").trigger("click");
    await flushPromises();

    const saved = mocks.invoke.mock.calls.find((c) => c[0] === "set_config_cmd")!;
    const items = JSON.parse(saved[1].value);
    expect(items[0].name).toBe("改后名称");
    expect(items[0].id).toBe(DEFAULT_PRESETS.reminders[0].id);
  });

  it("规则不完整时保存按钮禁用", async () => {
    const wrapper = await mountManager("reminders");
    await wrapper.find(".add-btn").trigger("click");
    // 默认相对 tab，但 time 为空且 n=1 h —— relative 合法（+1 小时）
    expect((wrapper.find(".btn-save").element as HTMLButtonElement).disabled).toBe(false);
    // 切到日历但不填日期 → 禁用
    await wrapper.findAll(".rule-tabs button")[1].trigger("click");
    expect((wrapper.find(".btn-save").element as HTMLButtonElement).disabled).toBe(true);
  });
});