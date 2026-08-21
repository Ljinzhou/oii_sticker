import { describe, it, expect, vi, beforeEach } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";

const mocks = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  confirmMock: vi.fn<(message: string) => boolean>(() => true),
}));

vi.mock("../../composables/useTauri", () => ({
  invoke: (...args: unknown[]) => mocks.invokeMock(...args),
}));

import WorkspaceManager from "./WorkspaceManager.vue";

type WsDto = { id: string; name: string; path: string; created_at: string };
type Handler = (args: Record<string, unknown> | undefined) => unknown;

const E = (id: string, name: string, path: string): WsDto => ({
  id,
  name,
  path,
  created_at: "1700000000",
});

const DEFAULT_WS = [E("w-1", "工作空间 A", "C:/ws/a"), E("w-2", "工作空间 B", "C:/ws/b")];
const DEFAULT_ROOT = "C:/Users/demo/Documents/oiistiker_workspace";

function setupBackend(opts: {
  currentId?: string | null;
  workspaces?: WsDto[];
  handlers?: Partial<Record<string, Handler>>;
} = {}) {
  const workspaces: WsDto[] = [...(opts.workspaces ?? DEFAULT_WS)];
  let currentId: string | null = "currentId" in opts ? (opts.currentId ?? null) : "w-1";

  mocks.invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
    const h = opts.handlers?.[cmd];
    if (h) return h(args) as Promise<unknown>;
    switch (cmd) {
      case "workspace_list_cmd":
        return Promise.resolve([...workspaces]);
      case "workspace_current_cmd":
        return Promise.resolve(workspaces.find((w) => w.id === currentId) ?? null);
      case "workspace_default_path_cmd":
        return Promise.resolve(DEFAULT_ROOT);
      case "workspace_switch_cmd":
        currentId = args?.id as string;
        return Promise.resolve();
      case "workspace_destroy_cmd": {
        const id = args?.id as string;
        const i = workspaces.findIndex((w) => w.id === id);
        if (i >= 0) workspaces.splice(i, 1);
        return Promise.resolve();
      }
      case "workspace_create_cmd": {
        const dto = E("w-new", (args?.name as string) || "未命名工作空间", args?.path as string);
        workspaces.push(dto);
        return Promise.resolve(dto);
      }
      case "workspace_backup_cmd":
        return Promise.resolve(1258000);
      case "workspace_transfer_cmd":
        return Promise.resolve("转移完成");
      default:
        return Promise.resolve(undefined);
    }
  });
}

async function mountManager() {
  const wrapper = mount(WorkspaceManager);
  await flushPromises();
  return wrapper;
}

beforeEach(() => {
  mocks.invokeMock.mockReset();
  mocks.confirmMock.mockReset();
  mocks.confirmMock.mockReturnValue(true);
  window.confirm = mocks.confirmMock as unknown as typeof window.confirm;
});

describe("WorkspaceManager", () => {
  it("挂载渲染当前 hero 卡与全部控件行，激活行高亮", async () => {
    setupBackend();
    const wrapper = await mountManager();

    const hero = wrapper.get(".ws-hero");
    expect(hero.text()).toContain("工作空间 A");
    expect(hero.text()).toContain("C:/ws/a");
    expect(hero.find(".ws-badge").exists()).toBe(true);
    expect(hero.find(".ws-badge").text()).toBe("当前");

    const rows = wrapper.findAll(".ws-row");
    expect(rows).toHaveLength(2);
    expect(rows[0].classes()).toContain("active");
    expect(rows[1].classes()).not.toContain("active");
    expect(rows[0].text()).toContain("当前");
    expect(rows[0].find(".row-switch").exists()).toBe(false);
    expect(rows[1].find(".row-switch").exists()).toBe(true);
  });

  it("切换前弹确认框，确认后调用 workspace_switch_cmd 并刷新列表", async () => {
    setupBackend();
    const wrapper = await mountManager();
    mocks.invokeMock.mockClear();

    const rows = wrapper.findAll(".ws-row");
    const switchBtn = rows[1].findAll("button").find((b) => b.text() === "切换")!;
    await switchBtn.trigger("click");
    await flushPromises();

    expect(mocks.confirmMock).toHaveBeenCalledTimes(1);
    expect(String(mocks.confirmMock.mock.calls[0][0])).toContain("工作空间 B");
    expect(mocks.invokeMock).toHaveBeenCalledWith("workspace_switch_cmd", { id: "w-2" });
    const listCalls = mocks.invokeMock.mock.calls.filter((c) => c[0] === "workspace_list_cmd");
    expect(listCalls).toHaveLength(1);

    expect(wrapper.findAll(".ws-row")[1].classes()).toContain("active");
  });

  it("取消确认弹窗时不做任何调用", async () => {
    setupBackend();
    const wrapper = await mountManager();
    mocks.invokeMock.mockClear();
    mocks.confirmMock.mockReturnValue(false);

    await wrapper.findAll(".ws-row")[1].findAll("button").find((b) => b.text() === "切换")!.trigger("click");
    await flushPromises();

    expect(mocks.confirmMock).toHaveBeenCalledTimes(1);
    expect(mocks.invokeMock.mock.calls.some((c) => c[0] === "workspace_switch_cmd")).toBe(false);
    expect(mocks.invokeMock.mock.calls.some((c) => c[0] === "workspace_list_cmd")).toBe(false);
  });

  it("销毁：确认后调用 workspace_destroy_cmd 并刷新", async () => {
    setupBackend();
    const wrapper = await mountManager();
    mocks.invokeMock.mockClear();

    const rows = wrapper.findAll(".ws-row");
    await rows[1].findAll("button").find((b) => b.text() === "销毁")!.trigger("click");
    await flushPromises();

    expect(mocks.invokeMock).toHaveBeenCalledWith("workspace_destroy_cmd", { id: "w-2" });
    const listCalls = mocks.invokeMock.mock.calls.filter((c) => c[0] === "workspace_list_cmd");
    expect(listCalls).toHaveLength(1);
    expect(wrapper.findAll(".ws-row")).toHaveLength(1);
  });

  it("销毁失败时显示错误 toast", async () => {
    setupBackend({
      handlers: {
        workspace_destroy_cmd: () => Promise.reject("不能删除当前激活的工作控件，请先切换到其他工作控件"),
      },
    });
    const wrapper = await mountManager();

    await wrapper.findAll(".ws-row")[1].findAll("button").find((b) => b.text() === "销毁")!.trigger("click");
    await flushPromises();

    const toast = wrapper.get(".ws-toast.error");
    expect(toast.text()).toContain("不能删除当前激活的工作控件");
    expect(wrapper.findAll(".ws-row")).toHaveLength(2);
  });

  it("备份：预填默认 zip 路径并调用 workspace_backup_cmd，展示大小", async () => {
    setupBackend();
    const wrapper = await mountManager();

    await wrapper.get(".hero-backup").trigger("click");
    const input = wrapper.get<HTMLInputElement>(".backup-input");
    expect(input.element.value).toBe(`${DEFAULT_ROOT}.zip`);

    await input.setValue("C:/backup/workspace.zip");
    await wrapper.get(".backup-run").trigger("click");
    await flushPromises();

    expect(mocks.invokeMock).toHaveBeenCalledWith("workspace_backup_cmd", {
      id: "w-1",
      destZip: "C:/backup/workspace.zip",
    });
    expect(wrapper.get(".ws-ok-line").text()).toContain("备份完成");
    expect(wrapper.get(".ws-ok-line").text()).toContain("1.2 MB");
  });

  it("转移：输入目标目录调用 workspace_transfer_cmd 并刷新", async () => {
    setupBackend();
    const wrapper = await mountManager();
    mocks.invokeMock.mockClear();

    await wrapper.findAll(".ws-row")[0].findAll("button").find((b) => b.text() === "转移")!.trigger("click");
    const input = wrapper.get<HTMLInputElement>(".transfer-input");
    expect(input.element.value).toBe("");
    await input.setValue("C:/ws/a-new");
    await wrapper.get(".transfer-run").trigger("click");
    await flushPromises();

    expect(mocks.invokeMock).toHaveBeenCalledWith("workspace_transfer_cmd", {
      id: "w-1",
      destRoot: "C:/ws/a-new",
    });
    const listCalls = mocks.invokeMock.mock.calls.filter((c) => c[0] === "workspace_list_cmd");
    expect(listCalls).toHaveLength(1);
  });

  it("新建：输入路径与名称调用 workspace_create_cmd 并刷新", async () => {
    setupBackend();
    const wrapper = await mountManager();

    await wrapper.get(".ws-new-trigger").trigger("click");
    await wrapper.get<HTMLInputElement>(".new-path-input").setValue("C:/ws/new");
    await wrapper.get<HTMLInputElement>(".new-name-input").setValue("新控件");
    await wrapper.get(".new-run").trigger("click");
    await flushPromises();

    expect(mocks.invokeMock).toHaveBeenCalledWith("workspace_create_cmd", {
      path: "C:/ws/new",
      name: "新控件",
    });
    expect(wrapper.findAll(".ws-row")).toHaveLength(3);
    expect(wrapper.text()).toContain("新控件");
  });

  it("刷新失败时显示错误 toast，完成后恢复可用", async () => {
    setupBackend({
      handlers: {
        workspace_list_cmd: () => Promise.reject("列表读取失败：数据库不可用"),
      },
    });
    const wrapper = await mountManager();

    const toast = wrapper.get(".ws-toast.error");
    expect(toast.text()).toContain("列表读取失败：数据库不可用");
    expect(wrapper.find(".ws-loading").exists()).toBe(false);
    expect(wrapper.get(".ws-refresh").attributes("disabled")).toBeUndefined();
  });

  it("异步操作进行中禁用全部按钮，完成后恢复", async () => {
    let release!: () => void;
    const gate = new Promise<void>((r) => {
      release = () => r();
    });
    setupBackend({
      handlers: {
        workspace_destroy_cmd: () => gate.then(() => undefined),
      },
    });
    const wrapper = await mountManager();

    await wrapper.findAll(".ws-row")[1].findAll("button").find((b) => b.text() === "销毁")!.trigger("click");
    await flushPromises();

    const buttons = wrapper.findAll("button.ws-btn");
    expect(buttons.length).toBeGreaterThan(0);
    expect(buttons.every((b) => b.attributes("disabled") !== undefined)).toBe(true);

    release();
    await flushPromises();
    expect(wrapper.findAll("button.ws-btn").some((b) => b.attributes("disabled") === undefined)).toBe(true);
  });
});
