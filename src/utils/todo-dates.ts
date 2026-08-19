import dayjs from "dayjs";

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
  if (key === "today") return toIso(now.hour(config.dueTodayHour).minute(0));
  if (key === "tomorrow") return toIso(now.add(1, "day").hour(config.dueTomorrowHour).minute(0));
  const days = (7 + config.dueNextWeekDow - now.day()) % 7 || 7;
  return toIso(now.add(days, "day").hour(config.dueTomorrowHour).minute(0));
}

export function formatTodoDate(value: string | null, withTime = true) {
  if (!value) return "未设置";
  return dayjs(value).format(withTime ? "M月D日 HH:mm" : "M月D日");
}
