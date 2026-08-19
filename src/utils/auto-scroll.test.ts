import { describe, expect, it } from "vitest";
import { advanceAutoScroll, type AutoScrollState } from "./auto-scroll";

function step(
  state: AutoScrollState,
  maxPosition: number,
  speed: number,
  deltaMs: number,
) {
  return advanceAutoScroll(state, maxPosition, speed, deltaMs);
}

describe("advanceAutoScroll", () => {
  it("按真实时间差向下推进", () => {
    expect(step({ position: 10, direction: 1 }, 100, 30, 100)).toEqual({
      position: 13,
      direction: 1,
    });
  });

  it("到达底部后反向并保留越界距离", () => {
    const next = step({ position: 99.8, direction: 1 }, 100, 5, 100);
    expect(next.position).toBeCloseTo(99.7, 8);
    expect(next.direction).toBe(-1);
  });

  it("到达顶部后反向", () => {
    expect(step({ position: 0.5, direction: -1 }, 100, 5, 100)).toEqual({
      position: 0,
      direction: 1,
    });
  });

  it("低速小数位移不会丢失", () => {
    const next = step({ position: 2, direction: 1 }, 100, 5, 16);
    expect(next.position).toBeCloseTo(2.08, 8);
  });

  it("长帧跨越多个边界仍保持往返方向", () => {
    expect(step({ position: 2, direction: 1 }, 10, 1000, 35)).toEqual({
      position: 3,
      direction: -1,
    });
  });

  it("无可滚动内容时归零并向下", () => {
    expect(step({ position: 12, direction: -1 }, 0, 30, 100)).toEqual({
      position: 0,
      direction: 1,
    });
  });

  it("无效速度或时间不产生位移", () => {
    expect(step({ position: 2, direction: 1 }, 10, -1, 100)).toEqual({
      position: 2,
      direction: 1,
    });
    expect(step({ position: 2, direction: 1 }, 10, 30, -1)).toEqual({
      position: 2,
      direction: 1,
    });
  });
});
