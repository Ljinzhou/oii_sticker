import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import { nextTick } from "vue";
import TodoDetail from "./TodoDetail.vue";
import RepeatPicker from "./RepeatPicker.vue";
import type { TodoBlock } from "../../types";
import { DEFAULT_PRESETS, parsePresetRule, presetToRepeatRule } from "../../utils/presets";
import dayjs from "dayjs";

// settings store 依赖 invoke（get_config_cmd 返回出厂默认预设）
const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("../../composables/useTauri", () => ({ invoke: invokeMock, listen: vi.fn() }));

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockResolvedValue({ entries: {} });
});

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

/** 当前块 id：parent_id 等于它就是「父任务」，等于别的父任务则是「子任务」。 */
const BLOCK = "block-1";

function mountDetail(item = makeTodo()) {
  return mount(TodoDetail, {
    props: { item, blockId: BLOCK },
    global: {
      plugins: [createPinia()],
      stubs: {
        TodoDatePicker: { name: "TodoDatePicker", template: "<div />" },
        RepeatPicker: { name: "RepeatPicker", template: "<div />" },
      },
    },
  });
}

function button(wrapper: ReturnType<typeof mountDetail>, text: string) {
  return wrapper.findAll("button").find((candidate) => candidate.text() === text)!;
}

function fieldButton(wrapper: ReturnType<typeof mountDetail>, label: string, text: string) {
  return wrapper.findAll(".picker-field").find((field) => field.find(".field-label").text().startsWith(label))!.findAll("button").find((candidate) => candidate.text() === text)!;
}

/** 出厂默认提醒预设的解析结果（与组件点击时一致的 now）。 */
function expectedIso(presetIndex: number, now: dayjs.Dayjs) {
  return parsePresetRule(DEFAULT_PRESETS.reminders[presetIndex].rule, now)!;
}

afterEach(() => vi.useRealTimers());

describe("TodoDetail preset selections（预设来自设置页列表）", () => {
  it("渲染预设列表按钮与「自定义」，点「1小时后」写入解析后的 ISO 并高亮", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-08-20T13:30:00+08:00"));
    const wrapper = mountDetail();
    await flushPromises();

    // 出厂默认按钮都在
    for (const item of DEFAULT_PRESETS.reminders) {
      expect(button(wrapper, item.name).exists()).toBe(true);
    }
    // 三行都以预设渲染 + 每行含自定义
    expect(wrapper.findAll(".chips button").length).toBe(DEFAULT_PRESETS.reminders.length + 1 + DEFAULT_PRESETS.due.length + 1 + DEFAULT_PRESETS.repeats.length + 1);

    await button(wrapper, "1小时后").trigger("click");
    const now = dayjs();
    expect(wrapper.emitted("patch")?.slice(-1)[0]).toEqual([{ reminder_at: expectedIso(0, now) }]);

    await wrapper.setProps({ item: makeTodo({ reminder_at: expectedIso(0, now) }) });
    expect(button(wrapper, "1小时后").classes()).toContain("active");
    expect(fieldButton(wrapper, "提醒时间", "自定义").classes()).not.toContain("active");
  });

  it("点「今天」截止预设写入当天 23:59，显示格式化时间", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-08-20T13:30:00+08:00"));
    const wrapper = mountDetail();
    await flushPromises();

    await button(wrapper, "今天").trigger("click");
    const now = dayjs();
    const iso = parsePresetRule(DEFAULT_PRESETS.due[0].rule, now)!;
    const patch = wrapper.emitted("patch")?.slice(-1)[0];
    expect(patch).toEqual([{ due_at: iso }]);
    expect(dayjs(iso).format("YYYY年M月D日 HH:mm")).toBe("2026年8月20日 23:59");

    await wrapper.setProps({ item: makeTodo({ due_at: iso }) });
    expect(button(wrapper, "今天").classes()).toContain("active");
    expect(wrapper.text()).toContain("截至时间 - 2026年8月20日 23:59");
  });

  it("点「每周」重复预设输出 RepeatPicker 兼容 JSON（按出厂规则）", async () => {
    const wrapper = mountDetail();
    await flushPromises();
    await button(wrapper, "每周").trigger("click");
    const rule = presetToRepeatRule(DEFAULT_PRESETS.repeats[1].rule);
    expect(wrapper.emitted("patch")?.slice(-1)[0]).toEqual([{ repeat_rule: rule }]);

    await wrapper.setProps({ item: makeTodo({ repeat_rule: rule! }) });
    expect(button(wrapper, "每周").classes()).toContain("active");
    await button(wrapper, "每周").trigger("click");
    expect(wrapper.emitted("patch")?.slice(-1)[0]).toEqual([{ repeat_rule: "" }]);
  });

  it("再次点击已生效的预设按钮取消设置", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-08-20T13:30:00+08:00"));
    const wrapper = mountDetail();
    await flushPromises();
    const now = dayjs();

    await button(wrapper, "1小时后").trigger("click");
    await wrapper.setProps({ item: makeTodo({ reminder_at: expectedIso(0, now) }) });
    expect(button(wrapper, "1小时后").classes()).toContain("active");

    await button(wrapper, "1小时后").trigger("click");
    expect(wrapper.emitted("patch")?.slice(-1)[0]).toEqual([{ reminder_at: "" }]);
    expect(button(wrapper, "1小时后").classes()).not.toContain("active");
  });

  it("切换任务时预设来源状态重置为自定义", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-08-20T13:30:00+08:00"));
    const wrapper = mountDetail();
    await flushPromises();

    await button(wrapper, "1小时后").trigger("click");
    await wrapper.setProps({ item: makeTodo({ reminder_at: expectedIso(0, dayjs()) }) });
    expect(button(wrapper, "1小时后").classes()).toContain("active");

    await wrapper.setProps({ item: makeTodo({ id: "todo-2", reminder_at: "2026-08-21T01:00:00.000Z" }) });
    expect(fieldButton(wrapper, "提醒时间", "自定义").classes()).toContain("active");
    expect(button(wrapper, "1小时后").classes()).not.toContain("active");
  });

  it("自定义已设置时再次点击自定义按钮取消设置", async () => {
    const wrapper = mountDetail(makeTodo({ due_at: "2026-08-21T00:00:00+08:00" }));
    await flushPromises();
    expect(fieldButton(wrapper, "截至时间", "自定义").classes()).toContain("active");

    await fieldButton(wrapper, "截至时间", "自定义").trigger("click");
    expect(wrapper.emitted("patch")?.slice(-1)[0]).toEqual([{ due_at: "" }]);
  });

  // 三层结构的关键回归：旧逻辑用 Boolean(parent_id) 判断"子任务"，
  // 会把父任务（parent_id = 块 id）误判成子任务而隐藏提醒设置。
  it("父任务（挂在块下）显示提醒/截止/重复，且可添加子任务", () => {
    const wrapper = mountDetail(makeTodo({ id: "p1", title: "父任务", parent_id: BLOCK }));
    expect(wrapper.text()).toContain("任务详情");
    expect(wrapper.text()).toContain("提醒时间");
    expect(wrapper.text()).toContain("截至时间");
    expect(wrapper.text()).toContain("设置任务重复时间");
    expect(wrapper.find(".add-child-btn").exists()).toBe(true);
    expect(wrapper.text()).not.toContain("子任务不继承高级设置");
  });

  it("子任务（挂在父任务下）隐藏提醒/截止/重复，只留名称与描述", () => {
    const wrapper = mountDetail(makeTodo({ id: "s1", title: "子任务", parent_id: "p1" }));
    expect(wrapper.text()).toContain("子任务详情");
    expect(wrapper.text()).not.toContain("提醒时间");
    expect(wrapper.text()).not.toContain("截至时间");
    expect(wrapper.text()).not.toContain("设置任务重复时间");
    expect(wrapper.find(".add-child-btn").exists()).toBe(false);
    expect(wrapper.text()).toContain("子任务不继承高级设置");
    expect(wrapper.find('input[type="text"]').exists()).toBe(true);
    expect(wrapper.find("textarea").exists()).toBe(true);
  });

  it("自定义按钮打开 Teleport 浮窗，点击外部关闭", async () => {
    const wrapper = mountDetail();
    await flushPromises();
    const customBtn = wrapper.findAll(".picker-field")[0].findAll("button").at(-1)!; // 提醒行最后一个 = 自定义
    await customBtn.trigger("click");
    expect(document.body.querySelector(".picker-float")).not.toBeNull();

    document.body.dispatchEvent(new Event("pointerdown", { bubbles: true }));
    await nextTick();
    expect(document.body.querySelector(".picker-float")).toBeNull();
    wrapper.unmount();
  });

  it("同一自定义按钮再次点击后浮窗保持关闭", async () => {
    const wrapper = mountDetail();
    await flushPromises();
    const customBtn = wrapper.findAll(".picker-field")[0].findAll("button").at(-1)!;
    await customBtn.trigger("click");
    expect(document.body.querySelector(".picker-float")).not.toBeNull();

    customBtn.element.dispatchEvent(new Event("pointerdown", { bubbles: true }));
    await customBtn.trigger("click");
    expect(document.body.querySelector(".picker-float")).toBeNull();
    wrapper.unmount();
  });

  it("RepeatPicker 保存的重复周期走自定义设置并可高亮", async () => {
    const wrapper = mountDetail();
    await flushPromises();
    await fieldButton(wrapper, "设置任务重复时间", "自定义").trigger("click");
    wrapper.getComponent(RepeatPicker).vm.$emit("save", JSON.stringify({ unit: "week", interval: 2, weekdays: [2, 1] }));
    await nextTick();
    await wrapper.setProps({ item: makeTodo({ repeat_rule: JSON.stringify({ unit: "week", interval: 2, weekdays: [2, 1] }) }) });
    expect(fieldButton(wrapper, "设置任务重复时间", "自定义").classes()).toContain("active");
    expect(wrapper.text()).toContain("设置任务重复时间 - 每 2 周的 周一、周二");
  });
});