import { afterEach, describe, expect, it } from "vitest";
import { showBootFailure } from "./boot-failure";

describe("showBootFailure", () => {
  afterEach(() => {
    document.body.innerHTML = "";
  });

  it("在应用挂载失败时把错误显示在启动占位区域", () => {
    document.body.innerHTML = '<div id="app"><p id="boot-fallback">正在启动...</p></div>';

    showBootFailure(new Error("Todo 模块加载失败"));

    expect(document.querySelector("#boot-fallback")?.textContent).toContain("Todo 模块加载失败");
  });

  it("启动占位已被清空时重新创建错误面板", () => {
    document.body.innerHTML = '<div id="app"></div>';

    showBootFailure("Todo 组件挂载失败");

    expect(document.querySelector("#boot-fallback")?.textContent).toContain("Todo 组件挂载失败");
  });
});
