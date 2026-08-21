import { afterEach, describe, expect, it, vi } from "vitest";
import { mount } from "@vue/test-utils";
import { nextTick } from "vue";
import TodoDetail from "./TodoDetail.vue";
import RepeatPicker from "./RepeatPicker.vue";
import type { TodoBlock } from "../../types";

const presets = {
  remindTomorrowHour: 9,
  remindNextWeekDow: 1,
  remindNextWeekHour: 9,
  dueTodayHour: 18,
  dueTomorrowHour: 9,
  dueNextWeekDow: 1,
};

function makeTodo(overrides: Partial<TodoBlock> = {}): TodoBlock {
  return {
    id: "todo-1",
    sticker_id: 1,
    title: "任务",
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

function mountDetail(item = makeTodo()) {
  return mount(TodoDetail, {
    props: { item, presets },
    global: {
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

afterEach(() => vi.useRealTimers());

describe("TodoDetail preset selections", () => {
  it("shows the reminder preset as selected and its summary in the field title", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-08-20T13:30:00+08:00"));
    const wrapper = mountDetail();

    await button(wrapper, "1小时后").trigger("click");
    await wrapper.setProps({ item: makeTodo({ reminder_at: "2026-08-20T06:30:00.000Z" }) });

    expect(button(wrapper, "1小时后").classes()).toContain("active");
    expect(fieldButton(wrapper, "提醒时间", "自定义").classes()).not.toContain("active");
    expect(wrapper.text()).toContain("提醒时间 - 2026年8月20日 14:30");
  });

  it("shows due and custom repeat selections with summaries", async () => {
    const wrapper = mountDetail();

    await button(wrapper, "今天").trigger("click");
    await wrapper.setProps({ item: makeTodo({ due_at: "2026-08-21T00:00:00+08:00" }) });
    expect(button(wrapper, "今天").classes()).toContain("active");
    expect(wrapper.text()).toContain("截至时间 - 2026年8月21日 00:00");

    await fieldButton(wrapper, "设置任务重复", "自定义").trigger("click");
    wrapper.getComponent(RepeatPicker).vm.$emit("save", JSON.stringify({ unit: "week", interval: 2, weekdays: [2, 1] }));
    await nextTick();
    await wrapper.setProps({ item: makeTodo({ repeat_rule: JSON.stringify({ unit: "week", interval: 2, weekdays: [2, 1] }) }) });

    expect(fieldButton(wrapper, "设置任务重复", "自定义").classes()).toContain("active");
    expect(wrapper.text()).toContain("设置任务重复 - 每 2 周的 周一、周二");
  });

  it("initializes stored values as custom and preserves a same-task selection after a patch replacement", async () => {
    const wrapper = mountDetail(makeTodo({ reminder_at: "2026-08-20T06:00:00.000Z" }));

    expect(fieldButton(wrapper, "提醒时间", "自定义").classes()).toContain("active");
    await button(wrapper, "1小时后").trigger("click");
    await wrapper.setProps({ item: makeTodo({ reminder_at: "2026-08-20T06:30:00.000Z" }) });

    expect(button(wrapper, "1小时后").classes()).toContain("active");
    expect(fieldButton(wrapper, "提醒时间", "自定义").classes()).not.toContain("active");
  });

  it("preserves due and repeat presets for same-task patches and resets another task's stored values to custom", async () => {
    const wrapper = mountDetail();

    await button(wrapper, "今天").trigger("click");
    await wrapper.setProps({ item: makeTodo({ due_at: "2026-08-21T00:00:00+08:00" }) });
    await button(wrapper, "每周").trigger("click");
    await wrapper.setProps({ item: makeTodo({ due_at: "2026-08-21T00:00:00+08:00", repeat_rule: JSON.stringify({ unit: "week", interval: 1 }) }) });

    expect(button(wrapper, "今天").classes()).toContain("active");
    expect(button(wrapper, "每周").classes()).toContain("active");

    await wrapper.setProps({ item: makeTodo({ id: "todo-2", due_at: "2026-08-22T00:00:00+08:00", repeat_rule: JSON.stringify({ unit: "day", interval: 1 }) }) });

    expect(fieldButton(wrapper, "截至时间", "自定义").classes()).toContain("active");
    expect(fieldButton(wrapper, "设置任务重复", "自定义").classes()).toContain("active");
    expect(button(wrapper, "今天").classes()).not.toContain("active");
    expect(button(wrapper, "每周").classes()).not.toContain("active");
  });

  it("自定义按钮打开 Teleport 浮窗，点击外部关闭", async () => {
    const wrapper = mount(TodoDetail, {
      props: { item: makeTodo(), presets },
      attachTo: document.body,
      global: { stubs: { TodoDatePicker: true, RepeatPicker: true } },
    });
    const customBtn = wrapper.findAll(".chips button")[3];
    await customBtn.trigger("click");

    expect(document.body.querySelector(".picker-float")).not.toBeNull();

    document.body.dispatchEvent(new Event("pointerdown", { bubbles: true }));
    await nextTick();
    expect(document.body.querySelector(".picker-float")).toBeNull();
    wrapper.unmount();
  });
});
