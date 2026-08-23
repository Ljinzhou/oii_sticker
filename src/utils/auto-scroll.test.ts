import { describe, expect, it } from "vitest";
import { createAutoScrollCursor, stepAutoScroll } from "./auto-scroll";

describe("stepAutoScroll", () => {
  it("按帧位移向下推进整数像素", () => {
    const r = stepAutoScroll(createAutoScrollCursor(1), 10, 100, 2);
    expect(r.scrollTop).toBe(12);
    expect(r.cursor.direction).toBe(1);
  });

  it("不足 1px 的速度累积 remainder，后续帧补齐为整像素", () => {
    const slow = { remainder: 0, direction: 1 as const };
    const a = stepAutoScroll(slow, 0, 100, 0.5);
    expect(a.scrollTop).toBe(0);
    expect(a.cursor.remainder).toBeCloseTo(0.5, 9);

    const b = stepAutoScroll(a.cursor, 0, 100, 0.5);
    expect(b.scrollTop).toBe(1);
    expect(b.cursor.remainder).toBeCloseTo(0, 9);
  });

  it("到达底部后折返向上，且不越过底部", () => {
    const r = stepAutoScroll(createAutoScrollCursor(1), 98, 100, 4);
    expect(r.scrollTop).toBe(100);
    expect(r.cursor.direction).toBe(-1);

    const up = stepAutoScroll(r.cursor, 100, 100, 10);
    expect(up.scrollTop).toBe(90);
    expect(up.cursor.direction).toBe(-1);
  });

  it("到达顶部后折返向下", () => {
    const r = stepAutoScroll(createAutoScrollCursor(-1), 2, 100, 4);
    expect(r.scrollTop).toBe(0);
    expect(r.cursor.direction).toBe(1);

    const down = stepAutoScroll(r.cursor, 0, 100, 10);
    expect(down.scrollTop).toBe(10);
    expect(down.cursor.direction).toBe(1);
  });

  it("长帧跨过底部：一次到位并换向", () => {
    const r = stepAutoScroll(createAutoScrollCursor(1), 95, 100, 30);
    expect(r.scrollTop).toBe(100);
    expect(r.cursor.direction).toBe(-1);
  });

  it("真实位置已越界（内容变短）：钳制到新范围并立即折返", () => {
    // 外部把内容改短后 scrollTop 停在旧值 120，而 max 只剩 80
    const r = stepAutoScroll(createAutoScrollCursor(1), 120, 80, 1);
    expect(r.scrollTop).toBe(79);
    expect(r.cursor.direction).toBe(-1);
  });

  it("内容不足一屏：归零待命（方向向下）", () => {
    const r = stepAutoScroll({ remainder: 0.6, direction: -1 }, 30, 0, 5);
    expect(r.scrollTop).toBe(0);
    expect(r.cursor).toEqual({ remainder: 0, direction: 1 });
  });

  it("速度为 0 或负数：不移动，保持方向", () => {
    const c = { remainder: 0.3, direction: -1 as const };
    const r = stepAutoScroll(c, 50, 100, 0);
    expect(r.scrollTop).toBe(50);
    expect(r.cursor.direction).toBe(-1);

    const r2 = stepAutoScroll(c, 50, 100, -3);
    expect(r2.scrollTop).toBe(50);
    expect(r2.cursor.direction).toBe(-1);
  });

  it("长内容多帧往复：到底折返、到顶再折返，永不越界", () => {
    let c = createAutoScrollCursor(1);
    let pos = 0;
    const seen: number[] = [];
    let dirChanges = 0;
    for (let i = 0; i < 60; i++) {
      const before = c.direction;
      const r = stepAutoScroll(c, pos, 100, 10);
      pos = r.scrollTop;
      c = r.cursor;
      seen.push(pos);
      if (c.direction !== before) dirChanges++;
    }
    // 在 0..100 之间往复，既不到顶停住，也不越过边界
    expect(Math.max(...seen)).toBe(100);
    expect(Math.min(...seen)).toBe(0);
    // 确实发生过多次方向折返（到底/到顶）
    expect(dirChanges).toBeGreaterThan(2);
  });
});
