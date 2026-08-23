import { describe, it, expect, vi, beforeEach } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import { createPinia } from "pinia";
import ConsoleView from "./ConsoleView.vue";

// —— mock Tauri IPC 层：捕获 invoke 调用与 listen 注册 ——
const mocks = vi.hoisted(() => {
  const handlers = new Map<string, (payload: unknown) => void>();
  return {
    handlers,
    invokeMock: vi.fn(),
    listenMock: vi.fn(async (event: string, handler: (p: unknown) => void) => {
      handlers.set(event, handler);
      return () => {};
    }),
  };
});

vi.mock("../../composables/useTauri", () => ({
  invoke: (...args: unknown[]) => mocks.invokeMock(...args),
  listen: (e: string, h: (p: unknown) => void) => mocks.listenMock(e, h),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ label: "main", minimize: vi.fn(), close: vi.fn() }),
}));

let nextStickerId = 100;
function mkSticker(id: number, group_id: number | null, title = `便签${id}`) {
  return {
    id,
    parent_id: null,
    group_id,
    title,
    content: "# 标题",
    heading_level: 0,
    pos_x: 200,
    pos_y: 140,
    width: 400,
    height: 500,
    opacity: 0.9,
    bg_color: null,
    always_on_top: false,
    auto_scroll: false,
    is_completed: false,
    display_mode: "edit",
    created_at: "",
    updated_at: "",
  };
}

function mkGroup(id: number, name: string) {
  return { id, name, sort_order: id, created_at: "" };
}

// 内存版数据库：invoke 处理器直接读写
const db = {
  stickers: [] as ReturnType<typeof mkSticker>[],
  groups: [] as ReturnType<typeof mkGroup>[],
  openIds: [1],
};

function setupInvoke(configEntries: Record<string, string> = {}) {
  mocks.invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "list_stickers_cmd":
        return Promise.resolve([...db.stickers]);
      case "group_list_cmd":
        return Promise.resolve([...db.groups]);
      case "list_open_sticker_ids_cmd":
        return Promise.resolve([...db.openIds]);
      case "get_config_cmd":
        return Promise.resolve({ entries: { ...configEntries } });
      case "set_config_cmd": {
        configEntries[args!.key as string] = args!.value as string;
        return Promise.resolve(undefined);
      }
      case "group_create_cmd": {
        const g = mkGroup(nextStickerId++, args!.name as string);
        db.groups.push(g);
        return Promise.resolve(g);
      }
      case "group_rename_cmd": {
        const g = db.groups.find((x) => x.id === args!.id);
        if (g) g.name = args!.name as string;
        return Promise.resolve(undefined);
      }
      case "group_delete_cmd": {
        const before = db.stickers.filter((s) => s.group_id === args!.id).length;
        db.groups = db.groups.filter((g) => g.id !== args!.id);
        if (args!.mode === "with-stickers") db.stickers = db.stickers.filter((s) => s.group_id !== args!.id);
        else db.stickers.forEach((s) => { if (s.group_id === args!.id) s.group_id = null; });
        return Promise.resolve(args!.mode === "with-stickers" ? before : 0);
      }
      default:
        return Promise.resolve(undefined);
    }
  });
}

async function mountConsole() {
  const wrapper = mount(ConsoleView, { global: { plugins: [createPinia()] } });
  await flushPromises();
  return wrapper;
}

beforeEach(() => {
  mocks.handlers.clear();
  mocks.invokeMock.mockReset();
  mocks.listenMock.mockClear();
  nextStickerId = 900;
  db.stickers = [
    mkSticker(1, null, "欢迎使用 oii_sticker"),
    mkSticker(2, 10, "工作便签"),
    mkSticker(3, 11, "生活便签"),
  ];
  db.groups = [mkGroup(10, "工作"), mkGroup(11, "生活")];
  db.openIds = [1];
  setupInvoke();
});

describe("ConsoleView", () => {
  it("卡片不渲染模式徽章（无 display_mode 文本标签）", async () => {
    const wrapper = await mountConsole();
    expect(wrapper.find(".mode-badge").exists()).toBe(false);
    expect(wrapper.text()).not.toContain("edit");
  });

  it("收到 push-update 事件后同时刷新列表与窗口打开状态", async () => {
    await mountConsole();
    mocks.invokeMock.mockClear();
    const handler = mocks.handlers.get("sticky://push-update");
    expect(handler).toBeTruthy();
    handler!(1);
    await flushPromises();
    const cmds = mocks.invokeMock.mock.calls.map((c) => c[0]);
    expect(cmds).toContain("list_stickers_cmd");
    expect(cmds).toContain("list_open_sticker_ids_cmd");
  });

  it("分区视图：默认分组在最前，其后按创建顺序排列分组", async () => {
    const wrapper = await mountConsole();
    const heads = wrapper.findAll(".group-head .group-name");
    expect(heads.map((h) => h.text())).toEqual(["默认分组", "工作", "生活"]);
    // 各分区计数徽标
    const counts = wrapper.findAll(".group-head .group-count");
    expect(counts.map((c) => c.text())).toEqual(["1", "1", "1"]);
    // 默认分组无 ⋯ 菜单按钮，非默认有
    expect(wrapper.findAll(".group-menu-btn")).toHaveLength(2);
  });

  it("视图切换持久化到 system_config（console_group_view）", async () => {    const wrapper = await mountConsole();
    const btns = wrapper.findAll(".view-switch button");
    await btns[1]!.trigger("click"); // 平铺
    let call = mocks.invokeMock.mock.calls.find((c) => c[0] === "set_config_cmd");
    expect(call).toBeTruthy();
    expect(call![1]).toMatchObject({ key: "console_group_view", value: "flat" });
    await btns[0]!.trigger("click"); // 分区
    call = mocks.invokeMock.mock.calls.filter((c) => c[0] === "set_config_cmd").slice(-1)[0];
    expect(call![1]).toMatchObject({ key: "console_group_view", value: "section" });
  });

  it("启动时在设置回读后恢复持久化的视图模式（flat）", async () => {
    setupInvoke({ console_group_view: "flat" });
    const wrapper = await mountConsole();
    const btns = wrapper.findAll(".view-switch button");
    expect(btns[1]!.classes()).toContain("on"); // 平铺激活
    expect(btns[0]!.classes()).not.toContain("on");
    // 平铺视图已生效：筛选 chips 可见
    expect(wrapper.find(".filter-chips").exists()).toBe(true);
  });

  it("平铺视图筛选 chips 过滤卡片", async () => {
    const wrapper = await mountConsole();
    await wrapper.findAll(".view-switch button")[1]!.trigger("click");
    // 全部
    let cards = wrapper.findAll(".cards .card");
    expect(cards).toHaveLength(3);
    // 点击「工作」chip → 只剩该组便签
    const workChip = wrapper.findAll(".filter-chips button").find((b) => b.text().startsWith("工作"))!;
    await workChip.trigger("click");
    cards = wrapper.findAll(".cards .card");
    expect(cards).toHaveLength(1);
    expect(wrapper.text()).toContain("工作便签");
    // 「默认分组」chip → 未分组便签
    const defChip = wrapper.findAll(".filter-chips button").find((b) => b.text().startsWith("默认分组"))!;
    await defChip.trigger("click");
    expect(wrapper.findAll(".cards .card")).toHaveLength(1);
    expect(wrapper.text()).toContain("欢迎使用 oii_sticker");
  });

  it("三选删除确认框：默认选中移回默认分组；连带删除需二次确认后调用 group_delete_cmd", async () => {
    const wrapper = await mountConsole();
    // 打开「工作」分组的 ⋯ 菜单 → 删除分组
    const menuBtn = wrapper.findAll(".group-menu-btn")[0]!;
    await menuBtn.trigger("click");
    await wrapper.find(".dropdown .danger-item").trigger("click");
    await flushPromises();

    expect(wrapper.find(".confirm-box").exists()).toBe(true);
    const toDefault = wrapper.find('input[value="to-default"]').element as HTMLInputElement;
    expect(toDefault.checked).toBe(true);

    // 选择「连同便签一起删除」，第一次点确认 → 出现红色警示行，按钮变「确认永久删除」
    await wrapper.find('input[value="with-stickers"]').setValue();
    expect(wrapper.find(".warn-line").exists()).toBe(false);
    const dangerBtn = wrapper.find(".confirm-box .btn.danger");
    expect(dangerBtn.text()).toBe("确认");
    await dangerBtn.trigger("click");
    expect(wrapper.find(".warn-line").exists()).toBe(true);
    expect(wrapper.find(".confirm-box .btn.danger").text()).toBe("确认永久删除");

    // 第二次点击 → 真正调用 group_delete_cmd（with-stickers）
    await wrapper.find(".confirm-box .btn.danger").trigger("click");
    await flushPromises();
    const delCall = mocks.invokeMock.mock.calls.find((c) => c[0] === "group_delete_cmd");
    expect(delCall).toBeTruthy();
    expect(delCall![1]).toEqual({ id: 10, mode: "with-stickers" });
    expect(wrapper.find(".confirm-box").exists()).toBe(false);
    expect(wrapper.text()).toContain("已删除分组及其内 1 张便签");
  });

  it("三选删除确认框：移回默认分组模式调用 group_delete_cmd(to-default)", async () => {
    const wrapper = await mountConsole();
    await wrapper.findAll(".group-menu-btn")[0]!.trigger("click");
    await wrapper.find(".dropdown .danger-item").trigger("click");
    await flushPromises();
    await wrapper.find(".confirm-box .btn.danger").trigger("click"); // 默认 radio，一次确认即执行
    await flushPromises();
    const delCall = mocks.invokeMock.mock.calls.find((c) => c[0] === "group_delete_cmd");
    expect(delCall![1]).toEqual({ id: 10, mode: "to-default" });
    expect(wrapper.text()).toContain("分组已删除，便签已移回默认分组");
  });

  it("新建分组调用 group_create_cmd", async () => {
    const wrapper = await mountConsole();
    await wrapper.find(".group-create button").trigger("click"); // ＋ 新建分组
    const input = wrapper.find(".group-create-input");
    expect(input.exists()).toBe(true);
    await input.setValue("学习");
    await wrapper.findAll(".group-create button").find((b) => b.text() === "确定")!.trigger("click");
    await flushPromises();
    const call = mocks.invokeMock.mock.calls.find((c) => c[0] === "group_create_cmd");
    expect(call).toBeTruthy();
    expect(call![1]).toEqual({ name: "学习" });
    // 新分组出现在分区列表
    const names = wrapper.findAll(".group-head .group-name").map((h) => h.text());
    expect(names).toContain("学习");
  });
});
