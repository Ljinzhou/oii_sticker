import dayjs from "dayjs";
import type { TodoBlock } from "../types";

export interface TodoPresetConfig {
  remindTomorrowHour: number;
  remindNextWeekDow: number;
  remindNextWeekHour: number;
  dueTodayHour: number;
  dueTomorrowHour: number;
  dueNextWeekDow: number;
}

export function toIso(value: dayjs.Dayjs) {
  return value.second(0).millisecond(0).toISOString();
}

export function reminderPreset(key: "hour" | "tomorrow" | "next-week", config: TodoPresetConfig): string {
  const now = dayjs();
  if (key === "hour") return toIso(now.add(1, "hour"));
  if (key === "tomorrow") return toIso(now.add(1, "day").hour(config.remindTomorrowHour).minute(0));
  const days = (7 + config.remindNextWeekDow - now.day()) % 7 || 7;
  return toIso(now.add(days, "day").hour(config.remindNextWeekHour).minute(0));
}

export function duePreset(key: "today" | "tomorrow" | "next-week", config: TodoPresetConfig): string {
  const now = dayjs();
  if (key === "today") return toIso(now.add(1, "day").startOf("day"));
  if (key === "tomorrow") return toIso(now.add(1, "day").hour(config.dueTomorrowHour).minute(0));
  const days = (7 + config.dueNextWeekDow - now.day()) % 7 || 7;
  return toIso(now.add(days, "day").hour(config.dueTomorrowHour).minute(0));
}

export function formatTodoDate(value: string | null, withTime = true) {
  if (!value) return "未设置";
  return dayjs(value).format(withTime ? "YYYY年M月D日 HH:mm" : "YYYY年M月D日");
}

/** 提醒高亮状态（渲染层共用）：
 *  - reminded：到点提醒已触发且任务未完成（后端 reminded_at 非空）；
 *  - overdue：已过截止时间且任务未完成。 */
export function todoHighlightState(
  block: Pick<TodoBlock, "due_at" | "is_completed" | "reminded_at" | "due_ack_at">,
): { reminded: boolean; overdue: boolean } {
  if (block.is_completed) return { reminded: false, overdue: false };
  const dueMs = block.due_at ? dayjs(block.due_at).valueOf() : NaN;
  return {
    // 提醒已触发（reminded_at 非空即高亮；确认时由 ack_todo_alert_cmd 清空该列）。
    reminded: Boolean(block.reminded_at),
    // 已过截止时间且用户未确认（due_ack_at 非空表示该时点已确认，不再高亮）。
    overdue: Number.isFinite(dueMs) && dueMs <= Date.now() && !Boolean(block.due_ack_at),
  };
}

export function formatTodoRepeat(value: string | null) {
  if (!value) return "未设置";
  try {
    const rule = JSON.parse(value) as { unit?: string; interval?: unknown; weekdays?: unknown };
    const unit = { day: "天", week: "周", month: "月", year: "年" }[rule.unit ?? ""];
    if (!unit) return "未设置";
    const interval = Object.prototype.hasOwnProperty.call(rule, "interval") ? rule.interval : 1;
    if (typeof interval !== "number" || !Number.isFinite(interval) || !Number.isInteger(interval) || interval <= 0) return "未设置";
    if (rule.weekdays !== undefined) {
      if (!Array.isArray(rule.weekdays) || new Set(rule.weekdays).size !== rule.weekdays.length || rule.weekdays.some((day) => typeof day !== "number" || !Number.isInteger(day) || day < 0 || day > 6)) return "未设置";
    }
    if (rule.unit === "week" && Array.isArray(rule.weekdays) && rule.weekdays.length) {
      const weekdayNames = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"];
      const weekdays = [...rule.weekdays].sort((a, b) =>
        [1, 2, 3, 4, 5, 6, 0].indexOf(a) - [1, 2, 3, 4, 5, 6, 0].indexOf(b),
      );
      return `每 ${interval} ${unit}的 ${weekdays.map((day) => weekdayNames[day === 0 ? 6 : day - 1]).join("、")}`;
    }
    return `每 ${interval} ${unit}`;
  } catch {
    return "未设置";
  }
}
