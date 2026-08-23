/** 滚动方向：1 = 向下滚动，-1 = 向上滚动。 */
export type AutoScrollDirection = 1 | -1;

/** 自动滚动游标：亚像素位移累积 + 当前方向。 */
export interface AutoScrollCursor {
  /** 未消费的亚像素位移（通常 0 ≤ remainder < 1） */
  remainder: number;
  direction: AutoScrollDirection;
}

export function createAutoScrollCursor(
  direction: AutoScrollDirection = 1,
): AutoScrollCursor {
  return { remainder: 0, direction };
}

export interface AutoScrollStep {
  /** 本帧应设置的整数 scrollTop（避免 WebView 对小数 scrollTop 的量化抖动） */
  scrollTop: number;
  /** 下一帧使用的游标（含折返后的新方向） */
  cursor: AutoScrollCursor;
}

/**
 * 按「真实滚动位置」推进一步自动滚动：
 *
 * - `current` 传 `el.scrollTop` 的真实值 —— 外部任何变化（布局、内容增删、
 *   用户/程序滚动）都会在下一帧自动跟随，不会像纯内存状态那样漂移失联；
 * - 位移可为小数，不足 1px 的部分累积进 `remainder`，慢速也持续前进；
 * - 到达底部（max）→ 自动折返向上；到达顶部（0）→ 自动折返向下，
 *   折返依据真实位置判定，不存在“到顶后停住”的死区。
 */
export function stepAutoScroll(
  cursor: Readonly<AutoScrollCursor>,
  current: number,
  max: number,
  stepPx: number,
): AutoScrollStep {
  // 无效位移（0 / 负 / NaN）：原地待命，保持方向
  if (!Number.isFinite(stepPx) || stepPx <= 0) {
    return { scrollTop: Math.round(current), cursor: { ...cursor } };
  }
  // 内容不足一屏：无可滚动范围，归零待命（方向向下）
  if (!Number.isFinite(max) || max <= 0) {
    return { scrollTop: 0, cursor: createAutoScrollCursor(1) };
  }

  const clampedCurrent = Math.max(
    0,
    Math.min(max, Number.isFinite(current) ? current : 0),
  );
  let direction: AutoScrollDirection = cursor.direction === -1 ? -1 : 1;
  let remainder = Number.isFinite(cursor.remainder) ? cursor.remainder : 0;

  // 边界折返：以真实位置为准。外部把内容改短后 scrollTop 停在旧值
  // （超出新 max）也会在这里被钳制并立即反向。
  if (clampedCurrent >= max && direction === 1) direction = -1;
  if (clampedCurrent <= 0 && direction === -1) direction = 1;

  const total = remainder + stepPx;
  const wholePx = Math.floor(total);
  remainder = total - wholePx;

  let target = clampedCurrent;
  if (wholePx > 0) {
    target =
      direction === 1
        ? Math.min(max, clampedCurrent + wholePx)
        : Math.max(0, clampedCurrent - wholePx);
  }

  // 命中边界 → 下一帧换向：往下滚到底折返向上，滚到顶折返向下
  let nextDirection = direction;
  if (target >= max) nextDirection = -1;
  if (target <= 0) nextDirection = 1;

  return {
    scrollTop: Math.round(target),
    cursor: { remainder, direction: nextDirection },
  };
}
