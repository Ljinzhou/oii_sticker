import { describe, expect, it } from "vitest";
import type { TodoBlockWithSticker } from "../types";
import { buildGroups, countStats, filterBlocks } from "./todo-overview";

function block(part: Partial<TodoBlockWithSticker>): TodoBlockWithSticker {
  return {
    id: "b0",
    sticker_id: 1,
    title: "",
    block_title: "",
    description: null,
    is_completed: false,
    parent_id: null,
    reminder_at: null,
    due_at: null,
    repeat_rule: null,
    created_at: "2026-09-01T00:00:00.000Z",
    updated_at: "2026-09-01T00:00:00.000Z",
    sticker_title: "便签",
    ...part,
  };
}

const NOW_MS = Date.now();
function isoFromNow(offsetMs: number): string {
  return new Date(NOW_MS + offsetMs).toISOString();
}

describe("buildGroups", () => {
  it("按 sticker_id 分组并保持输入顺序", () => {
    const blocks = [
      block({ id: "b1", sticker_id: 2, sticker_title: "学习" }),
      block({ id: "b2", sticker_id: 1, sticker_title: "工作", parent_id: "root" }),
      block({ id: "b3", sticker_id: 1, sticker_title: "工作" }),
    ];
    const groups = buildGroups(blocks);
    expect(groups.map((g) => g.stickerId)).toEqual([2, 1]);
    expect(groups.find((g) => g.stickerId === 1)?.items.map((i) => i.id)).toEqual(["b2", "b3"]);
  });

  it("按便签列表顺序重排分组，未出现的便签排最后", () => {
    const blocks = [
      block({ id: "b1", sticker_id: 4, sticker_title: "D" }),
      block({ id: "b2", sticker_id: 1, sticker_title: "A" }),
      block({ id: "b3", sticker_id: 2, sticker_title: "B" }),
    ];
    const groups = buildGroups(blocks, [2, 1]);
    expect(groups.map((g) => g.stickerId)).toEqual([2, 1, 4]);
  });
});

describe("countStats", () => {
  it("统计全部/待办/提醒中/逾期/已完成，提醒与逾期重叠按逾期计", () => {
    const blocks: TodoBlockWithSticker[] = [
      block({ id: "done", is_completed: true }),
      block({ id: "pending" }),
      block({ id: "reminded", reminded_at: isoFromNow(0) }),
      block({ id: "overdue", due_at: isoFromNow(-24 * 3600_000), reminded_at: isoFromNow(0) }),
    ];
    expect(countStats(blocks)).toEqual({ total: 4, pending: 1, reminded: 1, overdue: 1, done: 1 });
  });

  it("空数组全零", () => {
    expect(countStats([])).toEqual({ total: 0, pending: 0, reminded: 0, overdue: 0, done: 0 });
  });
});

describe("filterBlocks", () => {
  const todayDue = isoFromNow(6 * 3600_000); // 今天稍晚（未过期，仍属今天）
  const tomorrowDue = isoFromNow(24 * 3600_000);
  const pastDue = isoFromNow(-48 * 3600_000);
  const blocks: TodoBlockWithSticker[] = [
    block({ id: "today", title: "提交周报", due_at: todayDue }),
    block({ id: "tomorrow", title: "买牛奶", due_at: tomorrowDue }),
    block({ id: "past", title: "评审 PR", due_at: pastDue, reminded_at: pastDue }),
    block({ id: "done", title: "电费缴费", is_completed: true }),
  ];

  it("today：今天内截止或今天提醒", () => {
    const ids = filterBlocks(blocks, { today: true }).map((b) => b.id);
    expect(ids).toEqual(["today"]);
  });

  it("week：未来 7 天（含今天）", () => {
    const ids = filterBlocks(blocks, { week: true }).map((b) => b.id);
    expect(ids).toEqual(["today", "tomorrow"]);
  });

  it("alert：提醒已触发或已逾期", () => {
    const ids = filterBlocks(blocks, { alert: true }).map((b) => b.id);
    expect(ids).toEqual(["past"]);
  });

  it("done 精确匹配完成态", () => {
    expect(filterBlocks(blocks, { done: true }).map((b) => b.id)).toEqual(["done"]);
    expect(filterBlocks(blocks, { done: false }).map((b) => b.id)).toEqual(["today", "tomorrow", "past"]);
  });

  it("keyword 命中标题或便签标题（不区分大小写）", () => {
    const hits = filterBlocks(
      [
        block({ id: "a", title: "Review PR", sticker_title: "Work" }),
        block({ id: "b", title: "写周报", sticker_title: "工作" }),
      ],
      { keyword: "review" },
    );
    expect(hits.map((h) => h.id)).toEqual(["a"]);
  });

  it("条件组合为 AND", () => {
    const ids = filterBlocks(blocks, { done: false, week: true }).map((b) => b.id);
    expect(ids).toEqual(["today", "tomorrow"]);
  });
});