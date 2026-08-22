import { describe, it, expect, vi, beforeEach } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";

const mocks = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  pickDirMock: vi.fn<() => Promise<string | null>>(async () => null),
  saveMock: vi.fn<() => Promise<string | null>>(async () => null),
}));

vi.mock("../../composables/useTauri", () => ({
  invoke: (...args: unknown[]) => mocks.invokeMock(...args),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (...args: unknown[]) => mocks.pickDirMock(...(args as [])),
  save: (...args: unknown[]) => mocks.saveMock(...(args as [])),
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

/** 确认浮层：断言出现并点击「确定」/「取消」。 */
async function answerConfirmIn(wrapper: Awaited<ReturnType<typeof mountManager>>, yes: boolean, expectText?: string) {
  const mask = wrapper.get(".ws-confirm-mask");
  if (expectText) expect(mask.text()).toContain(expectText);
  const btn = mask.findAll("button").find((b) => b.classes().includes(yes ? "ws-confirm-ok" : "ws-confirm-cancel"))!;
  await btn.trigger("click");
  await flushPromises();
}

beforeEach(() => {
  mocks.invokeMock.mockReset();
  mocks.pickDirMock.mockReset();
  mocks.pickDirMock.mockResolvedValue(null);
  mocks.saveMock.mockReset();
  mocks.saveMock.mockResolvedValue(null);
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

  it("切换前弹确认浮层，确认后调用 workspace_switch_cmd 并刷新列表", async () => {
    setupBackend();
    const wrapper = await mountManager();
    mocks.invokeMock.mockClear();

    const rows = wrapper.findAll(".ws-row");
    const switchBtn = rows[1].findAll("button").find((b) => b.text() === "切换")!;
    await switchBtn.trigger("click");
    await flushPromises();
    await answerConfirmIn(wrapper, true, "工作空间 B");

    expect(mocks.invokeMock).toHaveBeenCalledWith("workspace_switch_cmd", { id: "w-2" });
    const listCalls = mocks.invokeMock.mock.calls.filter((c) => c[0] === "workspace_list_cmd");
    expect(listCalls).toHaveLength(1);

    expect(wrapper.findAll(".ws-row")[1].classes()).toContain("active");
  });

  it("取消确认浮窗时不做任何调用", async () => {
    setupBackend();
    const wrapper = await mountManager();
    mocks.invokeMock.mockClear();

    await wrapper.findAll(".ws-row")[1].findAll("button").find((b) => b.text() === "切换")!.trigger("click");
    await flushPromises();
    await answerConfirmIn(wrapper, false, "工作空间 B");

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
    await answerConfirmIn(wrapper, true, "无法恢复");

    expect(mocks.invokeMock).toHaveBeenCalledWith("workspace_destroy_cmd", { id: "w-2" });
    const listCalls = mocks.invokeMock.mock.calls.filter((c) => c[0] === "workspace_list_cmd");
    expect(listCalls).toHaveLength(1);
    expect(wrapper.findAll(".ws-row")).toHaveLength(1);
  });

  it("销毁失败时显示错误 toast", async () => {
    setupBackend({
      handlers: {
        workspace_destroy_cmd: () => Promise.reject("不能删除当前激活的工作空间，请先切换到其他工作空间"),
      },
    });
    const wrapper = await mountManager();

    await wrapper.findAll(".ws-row")[1].findAll("button").find((b) => b.text() === "销毁")!.trigger("click");
    await flushPromises();
    await answerConfirmIn(wrapper, true);

    const toast = wrapper.get(".ws-toast.error");
    expect(toast.text()).toContain("不能删除当前激活的工作空间");
    expect(wrapper.findAll(".ws-row")).toHaveLength(2);
  });

  it("备份：系统另存为对话框选择位置并调用 workspace_backup_cmd，展示大小", async () => {
    setupBackend();
    mocks.saveMock.mockResolvedValue("C:/backup/workspace.zip");
    const wrapper = await mountManager();
    mocks.invokeMock.mockClear();

    await wrapper.get(".hero-backup").trigger("click");
    await flushPromises();

    expect(mocks.saveMock).toHaveBeenCalledTimes(1);
    expect(mocks.invokeMock).toHaveBeenCalledWith("workspace_backup_cmd", {
      id: "w-1",
      destZip: "C:/backup/workspace.zip",
    });
    const okLine = wrapper.get(".ws-ok-line");
    expect(okLine.text()).toContain("备份完成");
    expect(okLine.text()).toContain("1.2 MB");
  });

  it("转移：目录对话框选择空文件夹直接调用 workspace_transfer_cmd 并刷新", async () => {
    setupBackend();
    mocks.pickDirMock.mockResolvedValue("C:/ws/a-new");
    const wrapper = await mountManager();
    mocks.invokeMock.mockClear();

    await wrapper.findAll(".ws-row")[0].findAll("button").find((b) => b.text() === "转移")!.trigger("click");
    await flushPromises();

    expect(mocks.pickDirMock).toHaveBeenCalledTimes(1);
    expect(wrapper.find(".ws-confirm-mask").exists()).toBe(false);
    expect(mocks.invokeMock).toHaveBeenCalledWith("workspace_transfer_cmd", {
      id: "w-1",
      destRoot: "C:/ws/a-new",
    });
    const listCalls = mocks.invokeMock.mock.calls.filter((c) => c[0] === "workspace_list_cmd");
    expect(listCalls).toHaveLength(1);
  });

  it("转移：目标非空时确认后改用 oiistiker_workspace 子目录重试", async () => {
    let transferCalls = 0;
    setupBackend({
      handlers: {
        workspace_transfer_cmd: (args) => {
          transferCalls += 1;
          if (args?.destRoot === "E:/") return Promise.reject(`DEST_NOT_EMPTY:E:/`);
          return Promise.resolve();
        },
      },
    });
    mocks.pickDirMock.mockResolvedValue("E:/");
    const wrapper = await mountManager();
    mocks.invokeMock.mockClear();

    await wrapper.findAll(".ws-row")[0].findAll("button").find((b) => b.text() === "转移")!.trigger("click");
    await flushPromises();
    await answerConfirmIn(wrapper, true, "oiistiker_workspace");

    expect(mocks.invokeMock).toHaveBeenCalledWith("workspace_transfer_cmd", {
      id: "w-1",
      destRoot: "E:/oiistiker_workspace",
    });
    expect(transferCalls).toBe(2);
  });

  it("新建：目录对话框选择路径与名称调用 workspace_create_cmd 并刷新", async () => {
    setupBackend();
    mocks.pickDirMock.mockResolvedValue("C:/ws/new");
    const wrapper = await mountManager();

    await wrapper.get(".ws-new-trigger").trigger("click");
    await flushPromises();
    // 所选路径回填展示
    expect(wrapper.get(".ws-picked-path").text()).toBe("C:/ws/new");
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

  it("新建：目标非空时确认后改用 oiistiker_workspace 子目录创建", async () => {
    setupBackend({
      handlers: {
        workspace_create_cmd: (args) => {
          if (args?.path === "E:/") return Promise.reject(`DEST_NOT_EMPTY:E:/`);
          const dto = E("w-new", "未命名工作空间", args?.path as string);
          return Promise.resolve(dto);
        },
      },
    });
    mocks.pickDirMock.mockResolvedValue("E:/");
    const wrapper = await mountManager();

    await wrapper.get(".ws-new-trigger").trigger("click");
    await flushPromises();
    await wrapper.get(".new-run").trigger("click");
    await flushPromises();
    await answerConfirmIn(wrapper, true, "oiistiker_workspace");

    expect(mocks.invokeMock).toHaveBeenCalledWith("workspace_create_cmd", {
      path: "E:/oiistiker_workspace",
      name: null,
    });
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
    await answerConfirmIn(wrapper, true);

    const buttons = wrapper.findAll("button.ws-btn");
    expect(buttons.length).toBeGreaterThan(0);
    expect(buttons.every((b) => b.attributes("disabled") !== undefined)).toBe(true);

    release();
    await flushPromises();
    expect(wrapper.findAll("button.ws-btn").some((b) => b.attributes("disabled") === undefined)).toBe(true);
  });
});
