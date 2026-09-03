// presets 工具单测：出厂默认、四类规则解析、中文描述、循环转换、逾期后缀剥离。
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import dayjs from "dayjs";
import {
  DEFAULT_PRESETS,
  defaultName,
  describePresetRule,
  makePresetItem,
  parsePresetRule,
  presetToRepeatRule,
  sanitizeRule,
  stripOverdueSuffix,
  type PresetRule,
} from "./presets";

// 固定 now：2026-08-21 周五 08:00（本地）
const NOW = "2026-08-21T08:00:00";

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

function at(iso: string) {
  vi.setSystemTime(new Date(iso));
}

describe("DEFAULT_PRESETS 出厂内置", () => {
  it("三组预设齐全且每条含 id/name/rule", () => {
    expect(DEFAULT_PRESETS.reminders.map((p) => p.name)).toEqual(["1小时后", "明天", "下周"]);
    expect(DEFAULT_PRESETS.due.map((p) => p.name)).toEqual(["今天", "明天", "下周"]);
    expect(DEFAULT_PRESETS.repeats.map((p) => p.name)).toEqual(["每天", "每周", "每月", "每年"]);
    for (const items of Object.values(DEFAULT_PRESETS)) {
      for (const item of items) {
        expect(item.id).toBeTruthy();
        expect(item.name).toBeTruthy();
        expect(item.rule.kind).toBeTruthy();
      }
    }
  });
});

describe("parsePresetRule 相对时间", () => {
  it("1小时后 → now+1h（秒毫秒截断）", () => {
    at(NOW);
    expect(parsePresetRule({ kind: "relative", n: 1, unit: "h", time: null })).toBe(
      dayjs(NOW).add(1, "hour").second(0).millisecond(0).toISOString(),
    );
  });

  it("带时刻的相对时间落在该时刻（明天 09:00）", () => {
    at(NOW);
    expect(parsePresetRule({ kind: "relative", n: 1, unit: "d", time: "09:00" })).toBe(
      dayjs(NOW).add(1, "day").hour(9).minute(0).second(0).millisecond(0).toISOString(),
    );
  });

  it("n=0 当天时刻（今天 23:59，延续旧截止语义）", () => {
    at(NOW);
    expect(parsePresetRule({ kind: "relative", n: 0, unit: "d", time: "23:59" })).toBe(
      dayjs(NOW).hour(23).minute(59).second(0).millisecond(0).toISOString(),
    );
  });

  it("2 小时后与 +1 周", () => {
    at(NOW);
    expect(parsePresetRule({ kind: "relative", n: 2, unit: "h", time: null })).toBe(
      dayjs(NOW).add(2, "hour").second(0).millisecond(0).toISOString(),
    );
    expect(parsePresetRule({ kind: "relative", n: 1, unit: "w", time: null })).toBe(
      dayjs(NOW).add(1, "week").second(0).millisecond(0).toISOString(),
    );
  });
});

describe("parsePresetRule 日历与星期", () => {
  it("日历时间 → 指定日期时刻", () => {
    at(NOW);
    expect(parsePresetRule({ kind: "calendar", date: "2026-12-25", time: "08:30" })).toBe(
      dayjs("2026-12-25T08:30:00").second(0).millisecond(0).toISOString(),
    );
  });

  it("下周一 09:00（周五起算）", () => {
    at(NOW); // 周五
    expect(parsePresetRule({ kind: "weekday", weekdays: [1], interval: 1, time: "09:00" })).toBe(
      dayjs("2026-08-24T09:00:00").toISOString(),
    );
  });

  it("工作日 18:30 → 下一个周一到周五", () => {
    at(NOW); // 周五，跳过周末
    expect(parsePresetRule({ kind: "weekday", weekdays: [1, 2, 3, 4, 5], interval: 1, time: "18:30" })).toBe(
      dayjs("2026-08-24T18:30:00").toISOString(),
    );
  });

  it("循环规则不是单次时间 → null", () => {
    at(NOW);
    expect(parsePresetRule({ kind: "cycle", interval: 1, unit: "day", weekdays: null })).toBeNull();
  });
});

describe("describePresetRule 中文描述", () => {
  it("相对时间描述（含今天/明天/后天/小时）", () => {
    expect(describePresetRule({ kind: "relative", n: 0, unit: "d", time: "23:59" })).toBe("今天 23:59");
    expect(describePresetRule({ kind: "relative", n: 1, unit: "d", time: "09:00" })).toBe("明天 09:00");
    expect(describePresetRule({ kind: "relative", n: 2, unit: "d", time: null })).toBe("后天");
    expect(describePresetRule({ kind: "relative", n: 3, unit: "h", time: null })).toBe("当前时间 +3 小时");
    expect(describePresetRule({ kind: "relative", n: 1, unit: "h", time: null })).toBe("当前时间 +1 小时");
  });

  it("日历/星期/循环描述", () => {
    expect(describePresetRule({ kind: "calendar", date: "2026-12-25", time: "08:30" })).toBe("2026年12月25日 08:30");
    expect(describePresetRule({ kind: "weekday", weekdays: [1, 3, 5], interval: 2, time: "09:00" })).toBe("每 2 周的 周一、周三、周五 09:00");
    expect(describePresetRule({ kind: "cycle", interval: 1, unit: "day", weekdays: null })).toBe("每 1 天");
    expect(describePresetRule({ kind: "cycle", interval: 1, unit: "week", weekdays: [2, 4] })).toBe("每 1 周的 周二、周四");
  });
});

describe("presetToRepeatRule 与 defaultName/makePresetItem", () => {
  it("循环预设 → RepeatPicker 兼容 JSON", () => {
    expect(presetToRepeatRule({ kind: "cycle", interval: 1, unit: "day", weekdays: null })).toBe(
      JSON.stringify({ unit: "day", interval: 1 }),
    );
    expect(presetToRepeatRule({ kind: "cycle", interval: 2, unit: "week", weekdays: [1, 3] })).toBe(
      JSON.stringify({ unit: "week", interval: 2, weekdays: [1, 3] }),
    );
  });

  it("非循环规则 → null", () => {
    expect(presetToRepeatRule({ kind: "relative", n: 1, unit: "h", time: null })).toBeNull();
  });

  it("未填名称时回退规则描述", () => {
    expect(defaultName({ kind: "relative", n: 1, unit: "d", time: null })).toBe("明天");
  });

  it("makePresetItem 生成唯一 id 与回退名", () => {
    const rule: PresetRule = { kind: "relative", n: 1, unit: "h", time: null };
    const a = makePresetItem("", rule, "reminders");
    const b = makePresetItem("", rule, "reminders");
    expect(a.name).toBe("当前时间 +1 小时");
    expect(a.id).not.toBe(b.id);
    expect(a.id.startsWith("r-")).toBe(true);
  });
});

describe("sanitizeRule 与逾期后缀", () => {
  it("非法输入返回 null", () => {
    expect(sanitizeRule(null)).toBeNull();
    expect(sanitizeRule({ kind: "nope" })).toBeNull();
    expect(sanitizeRule({ kind: "relative", n: -1, unit: "h", time: null })).toBeNull();
    expect(sanitizeRule({ kind: "calendar", date: "not-a-date", time: "09:00" })).toBeNull();
    expect(sanitizeRule({ kind: "weekday", weekdays: [], time: "x" })).toBeNull();
    expect(sanitizeRule({ kind: "cycle", interval: 1, unit: "century", weekdays: null })).toBeNull();
  });

  it("合法规则规整（去重、限位）", () => {
    expect(sanitizeRule({ kind: "weekday", weekdays: [1, 1, 9], time: "09:00", interval: 0 })).toEqual({
      kind: "weekday",
      weekdays: [1],
      interval: 1,
      time: "09:00",
    });
    expect(sanitizeRule({ kind: "relative", n: "2", unit: "d", time: "8:00" })).toEqual({
      kind: "relative",
      n: 2,
      unit: "d",
      time: null, // 非法时间串 → null
    });
  });

  it("逾期后缀追加与剥离", () => {
    expect(stripOverdueSuffix("背单词——2026年9月3日，任务逾期")).toBe("背单词");
    expect(stripOverdueSuffix("背单词")).toBe("背单词");
    expect(makePresetItem("", { kind: "cycle", interval: 1, unit: "day", weekdays: null }, "repeats").id.startsWith("p-")).toBe(true);
  });
});