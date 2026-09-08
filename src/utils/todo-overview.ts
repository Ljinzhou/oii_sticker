// 任务总览页纯函数：分组 / 筛选 / 统计（与 TodoHighlightState 共用 todo-dates 语义）
import dayjs from "dayjs";
import type { TodoBlockWithSticker } from "../types";
import { todoHighlightState } from "./todo-dates";

export interface TodoGroup {
  stickerId: number;
  stickerTitle: string;
  items: TodoBlockWithSticker[];
}

export interface TodoStats {
  total: number;
  pending: number;
  reminded: number;
  overdue: number;
  done: number;
}

/** 按 sticker_id 分组（items 保持后端层序）；可选按便签列表顺序重排分组。 */
export function buildGroups(blocks: TodoBlockWithSticker[], stickerOrder?: number[]): TodoGroup[] {
  const byId = new Map<number, TodoGroup>();
  for (const block of blocks) {
    let group = byId.get(block.sticker_id);
    if (!group) {
      group = { stickerId: block.sticker_id, stickerTitle: block.sticker_title, items: [] };
      byId.set(block.sticker_id, group);
    }
    group.items.push(block);
  }
  const groups = [...byId.values()];
  if (stickerOrder && stickerOrder.length > 0) {
    const rank = new Map(stickerOrder.map((id, index) => [id, index]));
    groups.sort(
      (a, b) =>
        (rank.get(a.stickerId) ?? Number.MAX_SAFE_INTEGER) -
        (rank.get(b.stickerId) ?? Number.MAX_SAFE_INTEGER),
    );
  }
  return groups;
}

/** 高亮统计：提醒中（橙）与已逾期（红）互斥，重叠时按逾期计（与列表展示口径一致）。 */
export function countStats(blocks: TodoBlockWithSticker[]): TodoStats {
  let reminded = 0;
  let overdue = 0;
  let done = 0;
  for (const block of blocks) {
    const hl = todoHighlightState(block);
    if (block.is_completed) done += 1;
    else if (hl.overdue) overdue += 1;
    else if (hl.reminded) reminded += 1;
  }
  return {
    total: blocks.length,
    pending: blocks.length - done - reminded - overdue,
    reminded,
    overdue,
    done,
  };
}

export interface TodoFilter {
  /** 截止/提醒在今天内（本地日界）。 */
  today?: boolean;
  /** 截止/提醒在未来 7 天内（含今天）。 */
  week?: boolean;
  /** 提醒已触发或已逾期且未确认。 */
  alert?: boolean;
  /** 已完成。 */
  done?: boolean;
  /** 标题 / 便签标题子串（不区分大小写）。 */
  keyword?: string;
}

function inDayRange(value: string | null, start: dayjs.Dayjs, end: dayjs.Dayjs): boolean {
  if (!value) return false;
  const ms = dayjs(value).valueOf();
  return Number.isFinite(ms) && ms >= start.valueOf() && ms <= end.valueOf();
}

/** 组合筛选（全部条件 AND）。today/week 只统计未完成任务的时间点（提醒/截止）。 */
export function filterBlocks(blocks: TodoBlockWithSticker[], filter: TodoFilter): TodoBlockWithSticker[] {
  const todayStart = dayjs().startOf("day");
  const todayEnd = dayjs().endOf("day");
  const weekEnd = dayjs().add(7, "day").endOf("day");
  const keyword = filter.keyword?.trim().toLowerCase();
  return blocks.filter((block) => {
    if (filter.done !== undefined && block.is_completed !== filter.done) return false;
    const hl = todoHighlightState(block);
    if (filter.alert && !(hl.reminded || hl.overdue)) return false;
    if (filter.today) {
      const inToday =
        inDayRange(block.due_at, todayStart, todayEnd) ||
        (inDayRange(block.reminder_at, todayStart, todayEnd) && !block.is_completed);
      if (!inToday) return false;
    }
    if (filter.week) {
      const inWeek =
        inDayRange(block.due_at, todayStart, weekEnd) ||
        (inDayRange(block.reminder_at, todayStart, weekEnd) && !block.is_completed);
      if (!inWeek) return false;
    }
    if (keyword) {
      const haystack = `${block.title} ${block.sticker_title}`.toLowerCase();
      if (!haystack.includes(keyword)) return false;
    }
    return true;
  });
}