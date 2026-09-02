import { afterEach, describe, expect, it, vi } from "vitest";
import dayjs from "dayjs";
import { duePreset, formatTodoDate, formatTodoRepeat, todoHighlightState } from "./todo-dates";

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
    expect(formatTodoRepeat(JSON.stringify({ unit: "day", interval: 3 }))).toBe("每 3 天");
    expect(formatTodoRepeat(JSON.stringify({ unit: "year", interval: 1 }))).toBe("每 1 年");
  });

  it("非法重复规则返回未设置", () => {
    expect(formatTodoRepeat("{")).toBe("未设置");
    expect(formatTodoRepeat(JSON.stringify({ unit: "hour", interval: 1 }))).toBe("未设置");
    for (const interval of [0, -1, "2", null, NaN, 1.5]) {
      expect(formatTodoRepeat(JSON.stringify({ unit: "day", interval }))).toBe("未设置");
    }
    for (const weekdays of [[1, 1], [1, 7], [1, 1.5], "1,2", null]) {
      expect(formatTodoRepeat(JSON.stringify({ unit: "week", interval: 1, weekdays }))).toBe("未设置");
    }
  });

  it("日期摘要包含完整年份和时间", () => {
    expect(formatTodoDate("2026-08-20T14:00:00+08:00")).toBe("2026年8月20日 14:00");
    expect(formatTodoDate("2026-08-20T14:00:00+08:00", false)).toBe("2026年8月20日");
  });
});

describe("todoHighlightState（提醒高亮状态）", () => {
  const base = { due_at: null as string | null, is_completed: false, reminded_at: null as string | null };

  it("reminded_at 非空 → reminded 高亮", () => {
    expect(todoHighlightState({ ...base, reminded_at: "2026-08-25T00:00:00Z" })).toEqual({ reminded: true, overdue: false });
  });

  it("截止时间已过且未完成 → overdue；未来截止不高亮", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-08-25T12:00:00Z"));
    expect(todoHighlightState({ ...base, due_at: "2026-08-25T11:59:59Z" })).toEqual({ reminded: false, overdue: true });
    // 恰好到点也算逾期
    expect(todoHighlightState({ ...base, due_at: "2026-08-25T12:00:00Z" }).overdue).toBe(true);
    expect(todoHighlightState({ ...base, due_at: "2026-08-25T12:00:01Z" }).overdue).toBe(false);
  });

  it("已确认截止（due_ack_at 非空）→ 已逾期不再高亮", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-08-25T12:00:00Z"));
    const acked = { ...base, due_at: "2026-08-25T10:00:00Z", due_ack_at: "2026-08-25T10:05:00Z" };
    expect(todoHighlightState(acked)).toEqual({ reminded: false, overdue: false });
  });

  it("已完成任务不参与任何高亮", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-08-25T12:00:00Z"));
    expect(todoHighlightState({ ...base, is_completed: true, reminded_at: "x", due_at: "2020-01-01T00:00:00Z" })).toEqual({ reminded: false, overdue: false });
  });
});
