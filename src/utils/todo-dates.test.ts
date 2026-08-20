import { afterEach, describe, expect, it, vi } from "vitest";
import dayjs from "dayjs";
import { duePreset, formatTodoDate, formatTodoRepeat } from "./todo-dates";

const config = {
  remindTomorrowHour: 9,
  remindNextWeekDow: 1,
  remindNextWeekHour: 9,
  dueTodayHour: 18,
  dueTomorrowHour: 9,
  dueNextWeekDow: 1,
};

afterEach(() => vi.useRealTimers());

describe("Todo 日期与重复格式化", () => {
  it("今天截止预设使用下一自然日零点", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-08-20T13:30:00+08:00"));
    expect(dayjs(duePreset("today", config)).format("YYYY-MM-DD HH:mm")).toBe("2026-08-21 00:00");
  });

  it("重复规则摘要包含间隔和按周一到周日排序的星期", () => {
    expect(formatTodoRepeat(JSON.stringify({ unit: "week", interval: 2, weekdays: [2, 1, 0] }))).toBe("每 2 周的 周一、周二、周日");
    expect(formatTodoRepeat(JSON.stringify({ unit: "month", interval: 1 }))).toBe("每 1 月");
  });

  it("日期摘要包含完整年份和时间", () => {
    expect(formatTodoDate("2026-08-20T14:00:00+08:00")).toBe("2026年8月20日 14:00");
  });
});
