// Todo 预设规则：定义、解析、描述与循环规则兼容转换。
//
// 预设是三组可增删改的条目（提醒时间 / 截至时间 / 重复），由「系统设置 → Todo 设置」
// 页管理，Todo 详情面板的三行 chips 按列表渲染。规则模型见设计文档
// docs/superpowers/specs/2026-09-03-todo-presets-design.md：
//   relative = 相对时间（+N 小时/天/周，可带固定时刻）
//   calendar = 具体日期 + 时刻
//   weekday  = 每周几 + 每 N 周 + 时刻（解析为「下一次匹配」的单次时间）
//   cycle    = 循环周期（仅重复预设；周期推进由后端 repeat_rule 承担）
import dayjs from "dayjs";
import { toIso } from "./todo-dates";

export type PresetKind = "reminders" | "due" | "repeats";

export interface PresetItem {
  id: string;
  name: string;
  rule: PresetRule;
}

export type PresetRule =
  | { kind: "relative"; n: number; unit: "h" | "d" | "w"; time: string | null }
  | { kind: "calendar"; date: string; time: string }
  | { kind: "weekday"; weekdays: number[]; interval: number; time: string }
  | { kind: "cycle"; interval: number; unit: "day" | "week" | "month" | "year"; weekdays: number[] | null };

const WEEKDAY_NAMES = ["周日", "周一", "周二", "周三", "周四", "周五", "周六"];
const UNIT_NAMES: Record<string, string> = { h: "小时", d: "天", w: "周" };
const CYCLE_NAMES: Record<string, string> = { day: "天", week: "周", month: "月", year: "年" };

function pad(n: number): string {
  return String(n).padStart(2, "0");
}

// ── 出厂内置预设（可增删改；未落库时作为缺省显示） ──

export const DEFAULT_PRESETS: Record<PresetKind, PresetItem[]> = {
  reminders: [
    { id: "r-1h", name: "1小时后", rule: { kind: "relative", n: 1, unit: "h", time: null } },
    { id: "r-tomorrow", name: "明天", rule: { kind: "relative", n: 1, unit: "d", time: "09:00" } },
    { id: "r-nextweek", name: "下周", rule: { kind: "weekday", weekdays: [1], interval: 1, time: "09:00" } },
  ],
  due: [
    // 「今天」= 当天 23:59（延续旧「下一自然日零点」的截止语义）
    { id: "d-today", name: "今天", rule: { kind: "relative", n: 0, unit: "d", time: "23:59" } },
    { id: "d-tomorrow", name: "明天", rule: { kind: "relative", n: 1, unit: "d", time: "09:00" } },
    { id: "d-nextweek", name: "下周", rule: { kind: "weekday", weekdays: [1], interval: 1, time: "09:00" } },
  ],
  repeats: [
    { id: "p-day", name: "每天", rule: { kind: "cycle", interval: 1, unit: "day", weekdays: null } },
    { id: "p-week", name: "每周", rule: { kind: "cycle", interval: 1, unit: "week", weekdays: [new Date().getDay()] } },
    { id: "p-month", name: "每月", rule: { kind: "cycle", interval: 1, unit: "month", weekdays: null } },
    { id: "p-year", name: "每年", rule: { kind: "cycle", interval: 1, unit: "year", weekdays: null } },
  ],
};

// ── 解析：规则 → 具体 ISO 时刻（提醒/截止用；循环规则返回 null） ──

export function parsePresetRule(rule: PresetRule, now: dayjs.Dayjs = dayjs()): string | null {
  switch (rule.kind) {
    case "relative": {
      const expr = now.add(rule.n, rule.unit);
      const base = rule.time
        ? expr.hour(Number(rule.time.slice(0, 2))).minute(Number(rule.time.slice(3, 5)))
        : expr;
      return toIso(base);
    }
    case "calendar": {
      const base = dayjs(`${rule.date}T${rule.time}:00`);
      return base.isValid() ? toIso(base) : null;
    }
    case "weekday": {
      // 「下一次匹配」：从明天起在 interval*7 天内找第一个命中的星期 + 时限
      const days = rule.weekdays.filter((d) => d >= 0 && d <= 6);
      if (days.length === 0) return null;
      const span = Math.max(1, Math.min(8, (rule.interval || 1) * 7));
      for (let offset = 1; offset <= span; offset++) {
        const candidate = now.add(offset, "day");
        if (days.includes(candidate.day())) {
          const at = candidate
            .hour(Number(rule.time.slice(0, 2)))
            .minute(Number(rule.time.slice(3, 5)))
            .second(0)
            .millisecond(0);
          return at.toISOString();
        }
      }
      return null;
    }
    case "cycle":
      return null; // 循环规则不是单次时间；由重复行使用
  }
}

// ── 描述：规则 → 中文摘要（设置页列表展示） ──

export function describePresetRule(rule: PresetRule): string {
  switch (rule.kind) {
    case "relative": {
      const t = rule.time ? ` ${rule.time}` : "";
      if (rule.n === 0) return rule.time ? `今天 ${rule.time}` : "当前时间";
      if (rule.n === 1 && rule.unit === "d") return rule.time ? `明天 ${rule.time}` : "明天";
      if (rule.n === 1 && rule.unit === "w") return rule.time ? `下周 ${rule.time}` : "下周";
      if (rule.n === 2 && rule.unit === "d") return rule.time ? `后天 ${rule.time}` : "后天";
      return `当前时间 +${rule.n} ${UNIT_NAMES[rule.unit] ?? rule.unit}${t}`;
    }
    case "calendar": {
      const d = dayjs(rule.date);
      return d.isValid() ? `${d.format("YYYY年M月D日")} ${rule.time}` : rule.date;
    }
    case "weekday": {
      const names = rule.weekdays.map((d) => WEEKDAY_NAMES[d]).join("、");
      const every = rule.interval > 1 ? `每 ${rule.interval} 周的 ` : "";
      return `${every}${names || "未选"} ${rule.time}`;
    }
    case "cycle": {
      const unit = CYCLE_NAMES[rule.unit] ?? rule.unit;
      if (rule.unit === "week" && rule.weekdays?.length) {
        const names = rule.weekdays.map((d) => WEEKDAY_NAMES[d]).join("、");
        return `每 ${rule.interval} ${unit}的 ${names}`;
      }
      return `每 ${rule.interval} ${unit}`;
    }
  }
}

// ── 转换：循环预设 → 后端 repeat_rule JSON（兼容 RepeatPicker 格式） ──

export function presetToRepeatRule(rule: PresetRule): string | null {
  if (rule.kind !== "cycle") return null;
  const json: { unit: string; interval: number; weekdays?: number[] } = {
    unit: rule.unit,
    interval: Math.max(1, rule.interval),
  };
  if (rule.unit === "week" && rule.weekdays?.length) {
    const days = rule.weekdays.filter((d) => d >= 0 && d <= 6);
    if (days.length) json.weekdays = [...new Set(days)];
  }
  return JSON.stringify(json);
}

/** 由 name/rule 构造预设条目（id 唯一化）。 */
export function makePresetItem(name: string, rule: PresetRule, kind: PresetKind, existing: PresetItem[]): PresetItem {
  const prefix = { reminders: "r", due: "d", repeats: "p" }[kind];
  const id = `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 6)}`;
  return { id, name: name.trim() || defaultName(rule), rule };
}

/** 未填名称时的回退名（取自规则描述）。 */
export function defaultName(rule: PresetRule): string {
  return describePresetRule(rule);
}

/** 校验并规整 rule（非法输入返回 null，供存储前兜底）。 */
export function sanitizeRule(raw: unknown): PresetRule | null {
  if (!raw || typeof raw !== "object") return null;
  const r = raw as Record<string, unknown>;
  switch (r.kind) {
    case "relative": {
      const n = Number(r.n);
      const unit = r.unit;
      if (!Number.isFinite(n) || n < 0 || n > 3650 || (unit !== "h" && unit !== "d" && unit !== "w")) return null;
      const time = typeof r.time === "string" && /^\d{2}:\d{2}$/.test(r.time) ? r.time : null;
      return { kind: "relative", n: Math.floor(n), unit, time };
    }
    case "calendar": {
      if (typeof r.date !== "string" || typeof r.time !== "string") return null;
      if (!dayjs(r.date).isValid() || !/^\d{2}:\d{2}$/.test(r.time)) return null;
      return { kind: "calendar", date: r.date, time: r.time };
    }
    case "weekday": {
      if (!Array.isArray(r.weekdays) || typeof r.time !== "string") return null;
      const days = r.weekdays.filter((d): d is number => Number.isInteger(d) && d >= 0 && d <= 6);
      if (days.length === 0 || !/^\d{2}:\d{2}$/.test(r.time)) return null;
      const interval = Math.max(1, Math.floor(Number(r.interval) || 1));
      return { kind: "weekday", weekdays: [...new Set(days)], interval, time: r.time };
    }
    case "cycle": {
      const interval = Math.max(1, Math.floor(Number(r.interval) || 1));
      const unit = r.unit;
      if (unit !== "day" && unit !== "week" && unit !== "month" && unit !== "year") return null;
      const weekdays = Array.isArray(r.weekdays)
        ? r.weekdays.filter((d): d is number => Number.isInteger(d) && d >= 0 && d <= 6)
        : null;
      return { kind: "cycle", interval, unit, weekdays: weekdays?.length ? [...new Set(weekdays)] : (unit === "week" ? null : null) };
    }
    default:
      return null;
  }
}

/** 从标题剥离逾期后缀（重建时取原始标题用）。 */
export const OVERDUE_SUFFIX_RE = /——\d{4}年\d{1,2}月\d{1,2}日，任务逾期$/;

export function stripOverdueSuffix(title: string): string {
  return title.replace(OVERDUE_SUFFIX_RE, "").trim();
}

export { pad };