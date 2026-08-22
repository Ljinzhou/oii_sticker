import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { mount, flushPromises, type VueWrapper } from "@vue/test-utils";
import { createPinia, setActivePinia, type Pinia } from "pinia";
import StickerCard from "./StickerCard.vue";
import { useNotesStore } from "../../stores/notes";
import type { Sticker } from "../../types";

// —— mock Tauri IPC 层（store 内 invoke）——
const mocks = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("../../composables/useTauri", () => ({
  invoke: (...args: unknown[]) => mocks.invokeMock(...args),
  listen: vi.fn(async () => () => {}),
}));

function mkSticker(id: number, group_id: number | null, title = `便签${id}`): Sticker {
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
    alert_active: false,
    display_mode: "edit",
    created_at: "",
    updated_at: "",
  };
}

function mkGroup(id: number, name: string) {
  return { id, name, sort_order: id, created_at: "" };
}

let pinia: Pinia;
const wrappers: VueWrapper[] = [];

beforeEach(() => {
  pinia = createPinia();
  setActivePinia(pinia);
  const notes = useNotesStore();
  notes.stickers = [mkSticker(1, null), mkSticker(2, 10)];
  notes.groups = [mkGroup(10, "工作"), mkGroup(11, "生活")];
  mocks.invokeMock.mockReset();
  mocks.invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "list_stickers_cmd") return Promise.resolve([...notes.stickers]);
    if (cmd === "group_list_cmd") return Promise.resolve([...notes.groups]);
    return Promise.resolve(undefined);
  });
});

afterEach(() => {
  wrappers.forEach((w) => w.unmount());
  wrappers.length = 0;
});

function mountCard(sticker: Sticker, isOpen = true): VueWrapper {
  const wrapper = mount(StickerCard, {
    props: { sticker, isOpen },
    global: { plugins: [pinia] },
    attachTo: document.body,
  });
  wrappers.push(wrapper);
  return wrapper;
}

async function openMenu(wrapper: VueWrapper) {
  await wrapper.find(".more").trigger("click");
}

describe("StickerCard 更多菜单", () => {
  it("⋯ 打开下拉菜单，菜单项齐全", async () => {
    const wrapper = mountCard(mkSticker(2, 10));
    expect(wrapper.find(".card-dropdown").exists()).toBe(false);

    await openMenu(wrapper);
    expect(wrapper.find(".card-dropdown").exists()).toBe(true);
    // 直接子按钮：重命名 / 转移分组 / 移出分组 / 删除便签
    const items = wrapper.findAll(".card-dropdown > button").map((b) => b.text());
    expect(items).toHaveLength(4);
    expect(items[0]).toContain("重命名");
    expect(items[1]).toContain("转移分组");
    expect(items[2]).toBe("移出分组");
    expect(items[3]).toContain("删除便签");

    // 再点 ⋯ 关闭（toggle 自身开合）
    await openMenu(wrapper);
    expect(wrapper.find(".card-dropdown").exists()).toBe(false);

    // 本卡其他区域（如显示/隐藏按钮）pointerdown 也关闭菜单
    await openMenu(wrapper);
    const showBtn = wrapper.findAll(".card-btns .btn")[1]!;
    showBtn.element.dispatchEvent(new Event("pointerdown", { bubbles: true }));
    await flushPromises();
    expect(wrapper.find(".card-dropdown").exists()).toBe(false);
  });

  it("跨卡片互斥：打开 A 菜单后按下 B 卡的 ⋯，A 先关闭、随后只打开 B", async () => {
    const a = mountCard(mkSticker(1, null, "甲卡"));
    const b = mountCard(mkSticker(2, 10, "乙卡"));

    await openMenu(a);
    expect(a.find(".card-dropdown").exists()).toBe(true);

    // pointerdown 命中 B 卡 ⋯（在 A 根元素之外）→ A 关闭
    const bMore = b.find(".more").element;
    bMore.dispatchEvent(new Event("pointerdown", { bubbles: true }));
    await flushPromises();
    expect(a.find(".card-dropdown").exists()).toBe(false);

    // B 的 click 正常打开自己的菜单（无双切换）
    await b.find(".more").trigger("click");
    expect(b.find(".card-dropdown").exists()).toBe(true);
    expect(a.find(".card-dropdown").exists()).toBe(false);

    // 反向：按 A 卡空白区域 → B 关闭
    a.find(".card-preview").element.dispatchEvent(new Event("pointerdown", { bubbles: true }));
    await flushPromises();
    expect(b.find(".card-dropdown").exists()).toBe(false);
  });

  it("重命名：Enter 提交调用 update_sticker_cmd，Esc 取消不提交", async () => {
    const s = mkSticker(1, null, "旧标题");
    useNotesStore().stickers[0] = s;
    const wrapper = mountCard(s);

    await openMenu(wrapper);
    await wrapper.findAll(".card-dropdown > button")[0]!.trigger("click"); // 重命名
    const input = wrapper.find<HTMLInputElement>(".card-title-edit");
    expect(input.exists()).toBe(true);
    expect(input.element.value).toBe("旧标题");

    await input.setValue("");
    await input.trigger("keydown.enter"); // 空标题不提交，直接退出编辑态
    await flushPromises();
    expect(mocks.invokeMock.mock.calls.some((c) => c[0] === "update_sticker_cmd")).toBe(false);
    expect(wrapper.find(".card-title-edit").exists()).toBe(false);

    // 重新进入重命名并提交
    await openMenu(wrapper);
    await wrapper.findAll(".card-dropdown > button")[0]!.trigger("click");
    const editInput = wrapper.find<HTMLInputElement>(".card-title-edit");
    await editInput.setValue("新标题");
    await editInput.trigger("keydown.enter");
    await flushPromises();
    const call = mocks.invokeMock.mock.calls.find((c) => c[0] === "update_sticker_cmd")!;
    expect(call![1]).toEqual({ id: s.id, patch: { title: "新标题" } });
    expect(wrapper.find(".card-title-edit").exists()).toBe(false);

    // Esc 取消：不产生第二次 update
    await openMenu(wrapper);
    await wrapper.findAll(".card-dropdown > button")[0]!.trigger("click");
    const input2 = wrapper.find<HTMLInputElement>(".card-title-edit");
    await input2.setValue("再改一次");
    await input2.trigger("keydown.esc");
    await flushPromises();
    expect(wrapper.find(".card-title-edit").exists()).toBe(false);
    const calls = mocks.invokeMock.mock.calls.filter((c) => c[0] === "update_sticker_cmd");
    expect(calls).toHaveLength(1);
  });

  it("转移分组子菜单只列出其他分组，点击调用 move_sticker_group_cmd", async () => {
    const s = mkSticker(2, 10); // 当前在「工作」组
    const wrapper = mountCard(s);

    await openMenu(wrapper);
    await wrapper.findAll(".card-dropdown > button")[1]!.trigger("click"); // 转移分组
    const subButtons = wrapper.findAll(".submenu button");
    // 只显示「生活」，不含当前所在「工作」
    expect(subButtons.map((b) => b.text())).toEqual(["生活"]);

    await subButtons[0]!.trigger("click");
    await flushPromises();
    const call = mocks.invokeMock.mock.calls.find((c) => c[0] === "move_sticker_group_cmd")!;
    expect(call![1]).toEqual({ stickerId: 2, groupId: 11 });
    expect(wrapper.find(".card-dropdown").exists()).toBe(false);
  });

  it("移出分组：默认组禁用不可点；非默认组点击后 groupId=null", async () => {
    // 默认组 → disabled
    const defSticker = mkSticker(1, null);
    let wrapper = mountCard(defSticker);
    await openMenu(wrapper);
    let btn = wrapper.findAll(".card-dropdown > button").find((b) => b.text() === "移出分组")!;
    expect(btn.attributes("disabled")).toBeDefined();
    await btn.trigger("click");
    await flushPromises();
    expect(
      mocks.invokeMock.mock.calls.filter((c) => c[0] === "move_sticker_group_cmd"),
    ).toHaveLength(0);
    wrapper.unmount();

    // 非默认组 → 可点
    const s = mkSticker(2, 10);
    wrapper = mountCard(s);
    await openMenu(wrapper);
    btn = wrapper.findAll(".card-dropdown > button").find((b) => b.text() === "移出分组")!;
    expect(btn.attributes("disabled")).toBeUndefined();
    await btn.trigger("click");
    await flushPromises();
    const call = mocks.invokeMock.mock.calls.find((c) => c[0] === "move_sticker_group_cmd")!;
    expect(call![1]).toEqual({ stickerId: 2, groupId: null });
    expect(wrapper.emitted("remove")).toBeUndefined();
  });

  it("删除便签 emit remove（父组件弹确认框），菜单关闭", async () => {
    const s = mkSticker(1, null);
    const wrapper = mountCard(s);

    await openMenu(wrapper);
    await wrapper.find(".card-dropdown .danger-item").trigger("click");
    expect(wrapper.find(".card-dropdown").exists()).toBe(false);
    const removed = wrapper.emitted("remove");
    expect(removed).toBeTruthy();
    expect((removed![0] as unknown[])[0]).toMatchObject({ id: s.id });
    expect(mocks.invokeMock.mock.calls.some((c) => c[0] === "delete_sticker_cmd")).toBe(false);
  });
});
