// update 进度辅助纯函数单测。
import { describe, expect, it } from "vitest";
import { downloadPercent, updateBarWidth, updateStageText } from "./update";

describe("updateBarWidth 整体进度", () => {
  it("下载阶段映射到 0-90%", () => {
    expect(updateBarWidth("downloading", 0, 100)).toBe(0);
    expect(updateBarWidth("downloading", 50, 100)).toBe(45);
    expect(updateBarWidth("downloading", 100, 100)).toBe(90); // 下载完成不越 90
  });

  it("安装/重启阶段固定 95/100", () => {
    expect(updateBarWidth("installing", null, null)).toBe(95);
    expect(updateBarWidth("restarting", null, null)).toBe(100);
  });

  it("非进度阶段与未知 total 返回 null（不确定）", () => {
    expect(updateBarWidth("checking", null, null)).toBeNull();
    expect(updateBarWidth("available", null, null)).toBeNull();
    expect(updateBarWidth("up_to_date", null, null)).toBeNull();
    expect(updateBarWidth("failed", null, null)).toBeNull();
    expect(updateBarWidth("downloading", 10, null)).toBeNull();
    expect(updateBarWidth("downloading", null, 100)).toBeNull();
  });
});

describe("updateStageText 阶段文案", () => {
  it("下载含百分比与重试标注", () => {
    expect(updateStageText("downloading", 45, false)).toBe("正在下载更新包… 45%");
    expect(updateStageText("downloading", 45, true)).toBe("下载中（已切换镜像重试） 45%");
    expect(updateStageText("downloading", null, false)).toBe("正在下载更新包…");
  });

  it("其余阶段文案", () => {
    expect(updateStageText("checking", null, false)).toBe("正在检查更新…");
    expect(updateStageText("installing", null, false)).toBe("正在安装…");
    expect(updateStageText("restarting", null, false)).toBe("安装完成，应用即将重启…");
    expect(updateStageText("available", null, false)).toBe("");
    expect(updateStageText("idle", null, false)).toBe("");
  });
});

describe("downloadPercent 按钮内百分比", () => {
  it("钳到 99% 不越界", () => {
    expect(downloadPercent(50, 100)).toBe(50);
    expect(downloadPercent(100, 100)).toBe(99);
    expect(downloadPercent(null, 100)).toBeNull();
    expect(downloadPercent(10, 0)).toBeNull();
  });
});