// Todo 提醒功能专项集成测试（垂直链路，走公共接口，不触碰内部实现）：
//
//   ① 设置提醒时间 / 截止时间（详情面板预设 + 自定义入口）
//   ② 到点触发：应用内 toast + 列表高亮（reminded 脉冲动画 / overdue 已逾期）
//   ③ 用户确认消除提醒状态：点击高亮行 → ack 落库 → 高亮消失 + just-acked 反馈动画
//
// mock 策略：把 Tauri invoke/listen 替换为「迷你后端」——内存 db 真实应用
// update/ack 的语义（patch 写回、ack 清触发标记并记确认时刻），事件按需手动
// 广播，从而用真实组件树验证用户可见行为，而不是测试函数签名。
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { createPinia } from "pinia";
import dayjs from "dayjs";
import type { TodoBlock } from "../../types";
import TodoWindow from "./TodoWindow.vue";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  onCloseRequested: vi.fn(),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    label: "todo-t-1",
    startDragging: vi.fn(),
    onCloseRequested: mocks.onCloseRequested,
  }),
}));

vi.mock("../../composables/useTauri", () => ({
  invoke: mocks.invoke,
  listen: mocks.listen,
}));

// ── 迷你后端：内存中的 todo_blocks 表 ──
// 三层结构：块(t-1) → 父任务(t-2) → 子任务(t-3)
const block = (): TodoBlock => ({
  id: "t-1", sticker_id: 7, title: "购物清单", block_title: "购物清单",
  description: null, is_completed: false, parent_id: null, reminder_at: null,
  due_at: null, repeat_rule: null, created_at: "", updated_at: "",
});
const task = (over: Partial<TodoBlock> = {}): TodoBlock => ({
  ...block(), id: "t-2", title: "购买牛奶", block_title: "", parent_id: "t-1", ...over,
});

interface MiniDb { blocks: TodoBlock[]; }

/** 把 TodoWindow 依赖的命令接到迷你后端上。 */
function serve(db: MiniDb) {
  mocks.invoke.mockImplementation((command: string, args: Record<string, unknown>) => {
    switch (command) {
      case "get_todo_block_cmd": {
        const id = args.id as string;
        return Promise.resolve(db.blocks.find((b) => b.id === id) ?? null);
      }
      case "list_todo_for_sticker_cmd": {
        const stickerId = args.stickerId as number;
        return Promise.resolve(db.blocks.filter((b) => b.sticker_id === stickerId));
      }
      case "update_todo_block_cmd": {
        // 与后端一致：TodoPatch 空串表示清空对应字段（reminder_at: "" → null）
        const patch = (args.patch ?? {}) as Record<string, unknown>;
        const target = db.blocks.find((b) => b.id === args.id);
        if (!target) return Promise.resolve(null);
        const next = { ...target } as TodoBlock & Record<string, unknown>;
        for (const [key, value] of Object.entries(patch)) {
          next[key] = value === "" ? null : value;
        }
        db.blocks = db.blocks.map((b) => (b.id === target.id ? (next as TodoBlock) : b));
        return Promise.resolve(next);
      }
      case "ack_todo_alert_cmd": {
        // 与后端 ack_alerts 语义一致：清触发标记，已触发字段记确认时刻
        const target = db.blocks.find((b) => b.id === args.id);
        if (!target) return Promise.resolve(null);
        const next: TodoBlock = {
          ...target,
          reminded_at: null,
          due_notified_at: null,
          reminder_ack_at: target.reminded_at ? new Date().toISOString() : target.reminder_ack_at,
          due_ack_at: target.due_notified_at ? new Date().toISOString() : target.due_ack_at,
        };
        db.blocks = db.blocks.map((b) => (b.id === target.id ? next : b));
        return Promise.resolve(next);
      }
      case "get_config_cmd":
        return Promise.resolve({ entries: {} });
      case "notify_todo_presence_cmd":
      case "notify_todo_saved_cmd":
      case "close_todo_window_cmd":
        return Promise.resolve(undefined);
      default:
        return Promise.resolve(undefined);
    }
  });
}

/** 事件通道：捕获两类监听（todo://updated 刷新 / todo://reminder-fired 提醒）。 */
const eventHandlers = new Map<string, (payload: unknown) => void>();
function installListenMock() {
  mocks.listen.mockImplementation((event: string, cb: (payload: unknown) => void) => {
    eventHandlers.set(event, cb);
    return Promise.resolve(() => eventHandlers.delete(event));
  });
}
installListenMock();

function broadcast(event: string, payload: unknown) {
  eventHandlers.get(event)?.(payload);
}

async function tick(ms: number) {
  await vi.advanceTimersByTimeAsync(ms);
}

async function mountWindow(): Promise<VueWrapper> {
  const wrapper = mount(TodoWindow, { global: { plugins: [createPinia()] } });
  await flushPromises();
  await tick(0); // initialize 内的异步监听就绪
  return wrapper;
}

/** 详情面板中「提醒时间」那一行的 chips 按钮（0=1小时后 1=明天 2=下周 3=自定义）。 */
function reminderButtons(wrapper: VueWrapper) {
  return wrapper.findAll(".picker-field")[0].findAll(".chips button");
}
/** 「截至时间」那一行的 chips 按钮（0=今天 1=明天 2=下周 3=自定义）。 */
function dueButtons(wrapper: VueWrapper) {
  return wrapper.findAll(".picker-field")[1].findAll(".chips button");
}
function fieldLabel(wrapper: VueWrapper, index: number) {
  return wrapper.findAll(".picker-field")[index].get(".field-label").text();
}
function row(wrapper: VueWrapper, id: string) {
  return wrapper.find(`.todo-list li[data-id="${id}"]`);
}

describe("Todo 提醒功能链路", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    mocks.invoke.mockReset();
    mocks.onCloseRequested.mockReset();
    mocks.listen.mockReset();
    installListenMock(); // mockReset 会清掉实现，需重新安装
    eventHandlers.clear();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  // ── ① 设置提醒时间 / 截止时间 ──

  it("点「1小时后」预设：写入合法 UTC ISO 的 reminder_at，面板显示格式化时间", async () => {
    const db: MiniDb = { blocks: [block(), task()] };
    serve(db);
    const wrapper = await mountWindow();

    await reminderButtons(wrapper)[0].trigger("click"); // 1小时后
    await tick(300); // patch 防抖 250ms 后落库

    const updateCalls = mocks.invoke.mock.calls.filter((c) => c[0] === "update_todo_block_cmd");
    expect(updateCalls).toHaveLength(1);
    const patch = updateCalls[0][1].patch as { reminder_at: string };
    expect(patch.reminder_at).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:00\.000Z$/);
    // 写回后详情面板展示「提醒时间 - 2026年…」并激活对应 chip
    await tick(0);
    expect(fieldLabel(wrapper, 0)).toMatch(/^提醒时间 - \d{4}年\d{1,2}月\d{1,2}日 \d{2}:\d{2}$/);
    expect(reminderButtons(wrapper)[0].classes()).toContain("active");
  });

  it("点「明天」预设：写入 due_at，面板显示「截至时间 - …」", async () => {
    const db: MiniDb = { blocks: [block(), task()] };
    serve(db);
    const wrapper = await mountWindow();

    await dueButtons(wrapper)[1].trigger("click"); // 明天
    await tick(300);

    const updateCalls = mocks.invoke.mock.calls.filter((c) => c[0] === "update_todo_block_cmd");
    expect(updateCalls).toHaveLength(1);
    expect((updateCalls[0][1].patch as { due_at: string }).due_at).toMatch(/.\.000Z$/);
    expect(fieldLabel(wrapper, 1)).toMatch(/^截至时间 - \d{4}年/);
  });

  it("自定义提醒时间可通过日期浮窗保存为 ISO 并显示", async () => {
    const db: MiniDb = { blocks: [block(), task()] };
    serve(db);
    const wrapper = await mountWindow();

    await reminderButtons(wrapper)[3].trigger("click"); // 自定义 → 浮窗
    await tick(0);
    const picker = wrapper.getComponent({ name: "TodoDatePicker" });
    picker.vm.$emit("save", "2030-01-02T03:04:00.000Z");
    await tick(300);

    const updateCalls = mocks.invoke.mock.calls.filter((c) => c[0] === "update_todo_block_cmd");
    expect((updateCalls[0][1].patch as { reminder_at: string }).reminder_at).toBe("2030-01-02T03:04:00.000Z");
    // 落库为 UTC ISO，展示按本地时区换算（跨时区稳定断言）
    expect(fieldLabel(wrapper, 0)).toContain(dayjs("2030-01-02T03:04:00.000Z").format("YYYY年M月D日 HH:mm"));
  });

  // ── ② 到点触发：应用内提示 + 高亮动画 ──

  it("到点触发提醒：弹应用内 toast，列表行进入 reminded 高亮（脉冲动画）", async () => {
    const db: MiniDb = { blocks: [block(), task({ reminder_at: "2026-08-25T02:00:00.000Z" })] };
    serve(db);
    const wrapper = await mountWindow();

    // 后端调度线程到点：标记已触发并广播
    db.blocks = db.blocks.map((b) =>
      b.id === "t-2" ? { ...b, reminded_at: "2026-08-25T02:00:00.000Z" } : b,
    );
    broadcast("todo://reminder-fired", {
      id: "t-2", sticker_id: 7, title: "购买牛奶", block_title: "",
      kind: "reminder", scheduled_at: "2026-08-25T02:00:00.000Z",
    });
    broadcast("todo://updated", "t-2"); // 通知前端刷新块数据
    await flushPromises();
    await tick(0);

    // 应用内提示文案
    expect(wrapper.find(".todo-toast").text()).toContain("提醒：购买牛奶");
    // 列表行高亮（reminded 类 = 触发脉冲动画样式）
    expect(row(wrapper, "t-2").classes()).toContain("reminded");
  });

  it("截止时间到点未确认：行进入 overdue 高亮并标注「已逾期」", async () => {
    const db: MiniDb = {
      blocks: [block(), task({ due_at: "2000-01-01T09:00:00.000Z", due_notified_at: "2000-01-01T09:00:00.000Z", due_ack_at: null })],
    };
    serve(db);
    const wrapper = await mountWindow();
    await tick(0);

    const li = row(wrapper, "t-2");
    expect(li.classes()).toContain("overdue");
    expect(li.classes()).not.toContain("reminded");
    // 「已逾期」标注由 CSS ::after 伪元素渲染，jsdom 取不到文本，以 overdue 类为准；
    // 同时确认触发标记（due_notified_at）驱动的 highlighted 状态不被误判为 reminded。
    expect(li.text()).toContain("购买牛奶");
  });

  it("提醒触发只响应所属便签（其他便签的提醒不弹提示、不高亮）", async () => {
    const db: MiniDb = { blocks: [block(), task({ reminder_at: "2026-08-25T02:00:00.000Z" })] };
    serve(db);
    const wrapper = await mountWindow();

    db.blocks = db.blocks.map((b) => (b.id === "t-2" ? { ...b, reminded_at: "2026-08-25T02:00:00.000Z" } : b));
    broadcast("todo://reminder-fired", {
      id: "t-2", sticker_id: 999, title: "别人的任务", block_title: "",
      kind: "reminder", scheduled_at: "2026-08-25T02:00:00.000Z",
    });
    await flushPromises();

    expect(wrapper.find(".todo-toast").exists()).toBe(false);
    expect(row(wrapper, "t-2").classes()).not.toContain("reminded");
  });

  // ── ③ 确认消除提醒状态 ──

  it("点击高亮行确认提醒：落库 ack，高亮消失并出现 just-acked 反馈动画", async () => {
    const db: MiniDb = {
      blocks: [block(), task({ reminder_at: "2026-08-25T02:00:00.000Z", reminded_at: "2026-08-25T02:00:00.000Z" })],
    };
    serve(db);
    const wrapper = await mountWindow();
    await tick(0);
    expect(row(wrapper, "t-2").classes()).toContain("reminded");

    await row(wrapper, "t-2").trigger("click");
    await flushPromises();

    // ack 命令落库（迷你后端清除触发标记）
    const ackCalls = mocks.invoke.mock.calls.filter((c) => c[0] === "ack_todo_alert_cmd");
    expect(ackCalls).toHaveLength(1);
    expect(ackCalls[0][1]).toEqual({ id: "t-2" });
    // 点击后本地立即给出「已确认提醒」反馈
    expect(wrapper.find(".todo-toast").text()).toContain("已确认提醒");

    // 后端推送刷新：高亮消失，出现 just-acked 脉冲动画类
    broadcast("todo://updated", "t-2");
    await flushPromises();
    await tick(0);
    const li = row(wrapper, "t-2");
    expect(li.classes()).not.toContain("reminded");
    expect(li.classes()).toContain("just-acked");

    // 反馈动画 900ms 后自动消失
    await tick(1000);
    expect(row(wrapper, "t-2").classes()).not.toContain("just-acked");
  });

  it("应用内提示在 2.5s 后自动消失", async () => {
    const db: MiniDb = { blocks: [block(), task({ reminder_at: "2026-08-25T02:00:00.000Z" })] };
    serve(db);
    const wrapper = await mountWindow();

    db.blocks = db.blocks.map((b) => (b.id === "t-2" ? { ...b, reminded_at: "2026-08-25T02:00:00.000Z" } : b));
    broadcast("todo://reminder-fired", {
      id: "t-2", sticker_id: 7, title: "购买牛奶", block_title: "",
      kind: "reminder", scheduled_at: "2026-08-25T02:00:00.000Z",
    });
    await flushPromises();
    expect(wrapper.find(".todo-toast").exists()).toBe(true);

    await tick(2600);
    expect(wrapper.find(".todo-toast").exists()).toBe(false);
  });
});