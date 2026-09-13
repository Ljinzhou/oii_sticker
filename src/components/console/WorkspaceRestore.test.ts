import { describe, it, expect, vi, beforeEach } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";

const mocks = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  dialogMock: vi.fn<(opts?: { directory?: boolean }) => Promise<string | null>>(async () => null),
}));

vi.mock("../../composables/useTauri", () => ({
  invoke: (...args: unknown[]) => mocks.invokeMock(...args),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (...args: unknown[]) => mocks.dialogMock(...(args as [{ directory?: boolean }?])),
}));

import WorkspaceRestore from "./WorkspaceRestore.vue";

const ZIP = "D:/backup/oiistiker_workspace.zip";
const DEST = "D:/restore-target";

const INFO = {
  format: 1,
  has_manifest: true,
  name: "工作空间 A",
  workspace_id: "w-1",
  created_at: "1700000000",
  backup_at_ms: Date.UTC(2026, 8, 13, 14, 30),
  app_version: "0.3.0",
  schema_version: 12,
  entries: 6,
  bytes: 2_500_000,
  zip_bytes: 1_258_000,
};

type Handler = (args: Record<string, unknown> | undefined) => unknown;

function setupBackend(handlers: Partial<Record<string, Handler>> = {}) {
  mocks.invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
    const h = handlers[cmd];
    if (h) return h(args) as Promise<unknown>;
    switch (cmd) {
      case "workspace_inspect_backup_cmd":
        return Promise.resolve(INFO);
      case "workspace_restore_cmd":
        return Promise.resolve({
          mode: (args?.mode as string) ?? "new",
          name: "工作空间 A",
          root: "D:/restore-target",
          entries: 6,
          workspace: null,
          rollback_zip: null,
          rollback_dir: null,
        });
      default:
        return Promise.resolve(undefined);
    }
  });
}

const WORKSPACES = [
  { id: "w-1", name: "工作空间 A", path: "C:/ws/a", created_at: "1" },
  { id: "w-2", name: "工作空间 B", path: "C:/ws/b", created_at: "2" },
];

function mountPanel() {
  return mount(WorkspaceRestore, {
    props: { workspaces: WORKSPACES, currentId: "w-1" },
  });
}

/** 打开面板 → 选中 zip → 进入摘要/方式选择步骤。 */
async function openWithBackup(wrapper: ReturnType<typeof mountPanel>) {
  await wrapper.get(".wsr-trigger").trigger("click");
  mocks.dialogMock.mockResolvedValueOnce(ZIP);
  await wrapper.findAll(".wsr-pick")[0].trigger("click");
  await flushPromises();
}

beforeEach(() => {
  mocks.invokeMock.mockReset();
  mocks.dialogMock.mockReset();
  mocks.dialogMock.mockResolvedValue(null);
});

describe("WorkspaceRestore（从备份恢复）", () => {
  it("入口打开面板并展示第一步；未选文件前不能提交", async () => {
    setupBackend();
    const wrapper = mountPanel();
    expect(wrapper.find(".wsr-panel").exists()).toBe(false);

    await wrapper.get(".wsr-trigger").trigger("click");
    const panel = wrapper.get(".wsr-panel");
    expect(panel.text()).toContain("选择备份文件");
    expect((wrapper.get(".wsr-submit").element as HTMLButtonElement).disabled).toBe(true);

    await wrapper.get(".wsr-x").trigger("click");
    expect(wrapper.find(".wsr-panel").exists()).toBe(false);
  });

  it("选择备份后展示摘要：名称、备份时间、条目数与体积", async () => {
    setupBackend();
    const wrapper = mountPanel();
    await openWithBackup(wrapper);

    expect(mocks.dialogMock).toHaveBeenCalledTimes(1);
    expect(mocks.invokeMock).toHaveBeenCalledWith("workspace_inspect_backup_cmd", { zipPath: ZIP });
    const meta = wrapper.get(".wsr-meta");
    expect(meta.text()).toContain("工作空间 A");
    expect(meta.text()).toContain("6 个");
    expect(meta.text()).toContain("2026-09-13");
    expect(meta.text()).toContain("1.2 MB"); // zip 体积
  });

  it("非备份文件：inspect 失败时展示错误且不能提交", async () => {
    setupBackend({
      workspace_inspect_backup_cmd: () => Promise.reject("备份缺少 workspace.json：不是本程序导出的工作空间备份"),
    });
    const wrapper = mountPanel();
    await openWithBackup(wrapper);

    expect(wrapper.get(".wsr-error").text()).toContain("不是本程序");
    expect(wrapper.find(".wsr-meta").exists()).toBe(false);
    expect((wrapper.get(".wsr-submit").element as HTMLButtonElement).disabled).toBe(true);
  });

  it("恢复为新工作空间：选目标目录后提交，参数与结果正确并通知父级刷新", async () => {
    setupBackend();
    const wrapper = mountPanel();
    await openWithBackup(wrapper);

    mocks.dialogMock.mockResolvedValueOnce(DEST);
    await wrapper.findAll(".wsr-pick")[1].trigger("click"); // 选择目标文件夹
    await flushPromises();
    expect(wrapper.text()).toContain(DEST);

    await wrapper.get(".wsr-submit").trigger("click");
    await flushPromises();

    expect(mocks.invokeMock).toHaveBeenCalledWith("workspace_restore_cmd", {
      zipPath: ZIP,
      mode: "new",
      destRoot: DEST,
      id: null,
      name: null,
    });
    expect(wrapper.get(".wsr-done").text()).toContain("已恢复");
    expect(wrapper.emitted("done")).toHaveLength(1);
  });

  it("覆盖模式：第一次点击只进入确认态，第二次才执行，并展示回滚点", async () => {
    setupBackend({
      workspace_restore_cmd: () =>
        Promise.resolve({
          mode: "overwrite",
          name: "工作空间 A",
          root: "C:/ws/a",
          entries: 6,
          workspace: null,
          rollback_zip: "C:/ws/a/cache/recovery-1.zip",
          rollback_dir: "C:/ws/a/cache/rollback-1.tmp",
        }),
    });
    const wrapper = mountPanel();
    await openWithBackup(wrapper);

    await wrapper.findAll(".wsr-mode")[1].trigger("click"); // 覆盖现有工作空间
    const submit = wrapper.get(".wsr-submit");
    expect(submit.text()).toContain("开始恢复");
    await submit.trigger("click");
    await flushPromises();
    // 只进入确认态，未发起调用
    expect(mocks.invokeMock.mock.calls.some((c) => c[0] === "workspace_restore_cmd")).toBe(false);
    expect(wrapper.get(".wsr-submit").text()).toContain("确认覆盖");

    await wrapper.get(".wsr-submit").trigger("click");
    await flushPromises();
    expect(mocks.invokeMock).toHaveBeenCalledWith("workspace_restore_cmd", {
      zipPath: ZIP,
      mode: "overwrite",
      destRoot: null,
      id: "w-1", // 默认覆盖当前工作空间
      name: null,
    });
    const done = wrapper.get(".wsr-done");
    expect(done.text()).toContain("recovery-1.zip");
    expect(done.text()).toContain("rollback-1.tmp");
  });

  it("覆盖失败：展示错误、重置确认态、不通知父级", async () => {
    setupBackend({
      workspace_restore_cmd: () => Promise.reject("恢复完成但重新打开数据库失败"),
    });
    const wrapper = mountPanel();
    await openWithBackup(wrapper);

    await wrapper.findAll(".wsr-mode")[1].trigger("click");
    await wrapper.get(".wsr-submit").trigger("click");
    await flushPromises();
    await wrapper.get(".wsr-submit").trigger("click");
    await flushPromises();

    expect(wrapper.get(".wsr-error").text()).toContain("重新打开数据库失败");
    expect(wrapper.find(".wsr-done").exists()).toBe(false);
    expect(wrapper.emitted("done")).toBeUndefined();
    // 确认态已复位，可再次尝试
    expect(wrapper.get(".wsr-submit").text()).toContain("开始恢复");
  });

  it("旧版备份（无 manifest）提示时间来源为文件修改时间", async () => {
    setupBackend({
      workspace_inspect_backup_cmd: () => Promise.resolve({ ...INFO, has_manifest: false, app_version: "" }),
    });
    const wrapper = mountPanel();
    await openWithBackup(wrapper);

    expect(wrapper.text()).toContain("旧版备份");
  });
});
