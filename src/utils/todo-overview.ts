// 任务总览页纯函数：分组 / 筛选 / 统计（与 TodoHighlightState 共用 todo-dates 语义）
import dayjs from "dayjs";
import type { TodoBlockWithSticker } from "../types";
import { todoHighlightState } from "./todo-dates";

export interface TodoBlockGroup {
  /** 所属 todo 块 id（第 0 层容器）。 */
  blockId: string;
  /** 块标题（空串 = 未命名块，由 UI 兜底展示）。 */
  blockTitle: string;
  /** 块内任务（父任务 + 子任务，保持后端层序：父任务在前、子任务紧随）。 */
  items: TodoBlockWithSticker[];
}

export interface TodoGroup {
  stickerId: number;
  stickerTitle: string;
  /** 该便签下的 todo 块（一个便签可有多个块）。 */
  blocks: TodoBlockGroup[];
  /** 便签内任务总数（含子任务，用于组头计数）。 */
  itemCount: number;
}

export interface TodoStats {
  total: number;
  pending: number;
  reminded: number;
  overdue: number;
  done: number;
}

/** 按「便签 → 块 → 任务」三层分组（层内保持后端顺序）；可选按便签列表顺序重排便签分组。
 *
 * 后端已按 便签 → 所属块 → 层序 排序，故这里只需顺序归并即可保持展示顺序稳定。
 * 块顺序沿用首次出现顺序（即块内首个任务的顺序）。
 */
export function buildGroups(blocks: TodoBlockWithSticker[], stickerOrder?: number[]): TodoGroup[] {
  const bySticker = new Map<number, TodoGroup>();
  // 复合键 `stickerId:blockId`——块 id 全局唯一，但按便签限定可避免任何脏数据导致的跨便签误合并
  const blockIndex = new Map<string, TodoBlockGroup>();
  for (const block of blocks) {
    let group = bySticker.get(block.sticker_id);
    if (!group) {
      group = { stickerId: block.sticker_id, stickerTitle: block.sticker_title, blocks: [], itemCount: 0 };
      bySticker.set(block.sticker_id, group);
    }
    const key = `${block.sticker_id}:${block.owner_block_id}`;
    let blockGroup = blockIndex.get(key);
    if (!blockGroup) {
      blockGroup = { blockId: block.owner_block_id, blockTitle: block.owner_block_title, items: [] };
      blockIndex.set(key, blockGroup);
      group.blocks.push(blockGroup);
    }
    blockGroup.items.push(block);
    group.itemCount += 1;
  }
  const groups = [...bySticker.values()];
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
  /** 标题 / 便签标题 / 所属块标题子串（不区分大小写）。 */
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
      // 命中范围含所属块标题：便于按「块标题」（如"本周计划"）检索整块任务
      const haystack = `${block.title} ${block.sticker_title} ${block.owner_block_title}`.toLowerCase();
      if (!haystack.includes(keyword)) return false;
    }
    return true;
  });
}