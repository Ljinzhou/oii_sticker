import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  isRelativeLocalPath,
  joinLocalPath,
  resetWorkspaceRootCache,
  resolveAssetSrc,
  resolveRelativeImages,
  workspaceRoot,
} from "./asset-url";

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("../composables/useTauri", () => ({
  invoke: (...args: unknown[]) => mocks.invoke(...args),
  convertFileSrc: (path: string) => `asset://localhost/${encodeURIComponent(path)}`,
}));

beforeEach(() => {
  mocks.invoke.mockReset();
  mocks.invoke.mockResolvedValue({ id: "w-1", path: "C:\\Users\\me\\Documents\\我的文档" });
  resetWorkspaceRootCache();
});

describe("isRelativeLocalPath", () => {
  it("相对路径判定为真", () => {
    expect(isRelativeLocalPath("assets/7/a.png")).toBe(true);
    expect(isRelativeLocalPath("a.png")).toBe(true);
    expect(isRelativeLocalPath("./assets/a.png")).toBe(true);
  });

  it("网络/协议/绝对路径判定为假", () => {
    for (const src of [
      "https://x/a.png",
      "http://x/a.png",
      "data:image/png;base64,AAA",
      "blob:http://localhost/x",
      "asset://localhost/x",
      "//host/a.png",
      "/abs/a.png",
      "\\abs\\a.png",
      "C:\\Users\\a.png",
      "",
    ]) {
      expect(isRelativeLocalPath(src), src).toBe(false);
    }
  });
});

describe("joinLocalPath", () => {
  it("拼接工作空间根目录，统一成反斜杠且去掉多余分隔符", () => {
    expect(joinLocalPath("C:\\ws\\", "assets/7/a.png")).toBe("C:\\ws\\assets\\7\\a.png");
    expect(joinLocalPath("C:\\ws", "./assets/./7/a.png")).toBe("C:\\ws\\assets\\7\\a.png");
  });
});

describe("workspaceRoot / resolveAssetSrc", () => {
  it("相对路径 → asset URL（含 %20 解码）", async () => {
    expect(await resolveAssetSrc("assets/7/粘贴-1.png")).toContain("asset://localhost/");
    expect(await resolveAssetSrc("assets/7/a%20b.png")).toContain(encodeURIComponent("a b.png"));
  });

  it("非相对路径原样返回，且不查工作空间", async () => {
    expect(await resolveAssetSrc("https://x/a.png")).toBe("https://x/a.png");
    expect(await resolveAssetSrc("data:image/png;base64,AA")).toBe("data:image/png;base64,AA");
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("工作空间根目录只查一次（模块级缓存）", async () => {
    await workspaceRoot();
    await workspaceRoot();
    expect(mocks.invoke).toHaveBeenCalledTimes(1);
    expect(mocks.invoke).toHaveBeenCalledWith("workspace_current_cmd");
  });

  it("拿不到工作空间时原样返回（不抛异常打断渲染）", async () => {
    mocks.invoke.mockResolvedValue(null);
    resetWorkspaceRootCache();
    expect(await resolveAssetSrc("assets/7/a.png")).toBe("assets/7/a.png");
  });
});

describe("resolveRelativeImages", () => {
  it("只替换相对路径的 img，其它 img 与无 src 的不动", async () => {
    const host = document.createElement("div");
    host.innerHTML = `
      <img id="local" src="assets/7/a.png">
      <img id="remote" src="https://x/b.png">
      <img id="absolute" src="C:/c.png">
    `;
    await resolveRelativeImages(host);
    expect(host.querySelector<HTMLImageElement>("#local")!.getAttribute("src")).toContain("asset://localhost/");
    expect(host.querySelector<HTMLImageElement>("#remote")!.getAttribute("src")).toBe("https://x/b.png");
    expect(host.querySelector<HTMLImageElement>("#absolute")!.getAttribute("src")).toBe("C:/c.png");
  });

  it("没有容器/没有图片时不报错", async () => {
    await expect(resolveRelativeImages(null)).resolves.toBeUndefined();
    await expect(resolveRelativeImages(document.createElement("div"))).resolves.toBeUndefined();
  });
});
