// StickerPreview：统计信息全部在上 → 一条分割线 → 分割线以下全部是正文预览内容。
import { describe, it, expect, beforeEach, vi } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import StickerPreview from "./StickerPreview.vue";
import type { Sticker } from "../../types";

// 面板会拉取 todo 块来渲染任务卡片与统计：mock IPC 层
const mocks = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("../../composables/useTauri", () => ({
  invoke: (...args: unknown[]) => mocks.invokeMock(...args),
  listen: vi.fn(async () => () => {}),
}));

function mkSticker(over: Partial<Sticker> = {}): Sticker {
  return {
    id: 1,
    parent_id: null,
    group_id: null,
    uid: "8f3a91c2",
    title: "项目周报",
    content: "# 本周进展\n\n- [ ] 待办事项\n- [x] 已完成事项",
    heading_level: 0,
    pos_x: 0,
    pos_y: 0,
    width: 400,
    height: 500,
    opacity: 0.9,
    bg_color: null,
    always_on_top: false,
    auto_scroll: false,
    is_completed: false,
    display_mode: "view",
    created_at: "",
    updated_at: "2026-09-16 20:12",
    ...over,
  } as unknown as Sticker;
}

beforeEach(() => {
  setActivePinia(createPinia());
  mocks.invokeMock.mockReset();
  mocks.invokeMock.mockResolvedValue([]);
});

describe("StickerPreview 结构", () => {
  it("统计在上、分割线、分割线以下只有正文预览（无路径行 / 结构统计）", () => {
    const wrapper = mount(StickerPreview, { props: { sticker: mkSticker() } });
    const html = wrapper.html();
    const iMetrics = html.indexOf('class="metrics"');
    const iBody = html.indexOf('class="pv-body"');
    expect(iMetrics).toBeGreaterThanOrEqual(0);
    expect(iBody).toBeGreaterThan(iMetrics); // 统计卡之后才是正文（pv-body 上边框即分割线）

    const body = wrapper.find(".pv-body");
    expect(body.find(".md-body").exists()).toBe(true);
    expect(body.text()).not.toContain("结构统计");
    expect(body.find(".kv").exists()).toBe(false); // 旧的结构统计列表已移除

    // 路径行与结构统计小字已按需求移除
    expect(wrapper.find(".pv-path").exists()).toBe(false);
    expect(wrapper.find(".meta-line").exists()).toBe(false);
  });

  it("正文渲染结果落在分割线以下", () => {
    const wrapper = mount(StickerPreview, { props: { sticker: mkSticker() } });
    expect(wrapper.find(".pv-body .md-body").html()).toContain("待办事项");
  });

  it("未选中便签时显示空态", () => {
    const wrapper = mount(StickerPreview, { props: { sticker: null } });
    expect(wrapper.find(".pv-blank").exists()).toBe(true);
  });
});


describe("StickerPreview todo 块", () => {
  it("渲染 todo 块任务卡片（不再出现「未找到任务」）并统计任务数", async () => {
    mocks.invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_all_todos_cmd") {
        return [
          // 第 0 层块容器（parent_id = null，本身不算任务）
          { id: "blk-1", sticker_id: 1, parent_id: null, title: "块", block_title: "块", is_completed: false },
          { id: "t-1", sticker_id: 1, parent_id: "blk-1", title: "待办任务", block_title: "块", is_completed: false },
          { id: "t-2", sticker_id: 1, parent_id: "blk-1", title: "已完成任务", block_title: "块", is_completed: true },
          // 别的便签的任务不应计入
          { id: "t-9", sticker_id: 2, parent_id: "blk-2", title: "他人任务", block_title: "块", is_completed: false },
        ];
      }
      return undefined;
    });

    const wrapper = mount(StickerPreview, {
      props: {
        sticker: mkSticker({
          content: `# 计划

<todo-block id="blk-1"></todo-block>`,
        }),
      },
    });
    await flushPromises();

    // 正文里渲染出任务卡片，而不是「未找到任务」占位
    expect(wrapper.find(".pv-body .todo-block-card").exists()).toBe(true);
    expect(wrapper.find(".pv-body .todo-block-missing").exists()).toBe(false);
    expect(wrapper.find(".pv-body").text()).not.toContain("未找到任务");
    expect(wrapper.find(".pv-body .todo-task-checkbox").exists()).toBe(true);

    // 统计只算本便签的任务：1 项待完成 / 1 项已完成
    const values = wrapper.findAll(".metric .v").map((v) => v.text());
    expect(values[2]).toBe("1项");
    expect(values[3]).toBe("1项");
  });

  it("GFM 复选框与 todo 块任务合并计数", async () => {
    mocks.invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_all_todos_cmd") {
        return [{ id: "t-1", sticker_id: 1, parent_id: "blk-1", title: "块任务", block_title: "块", is_completed: false }];
      }
      return undefined;
    });

    const wrapper = mount(StickerPreview, {
      props: {
        sticker: mkSticker({
          content: `- [ ] 列表任务

<todo-block id="blk-1"></todo-block>`,
        }),
      },
    });
    await flushPromises();

    const values = wrapper.findAll(".metric .v").map((v) => v.text());
    expect(values[2]).toBe("2项"); // 1 个 todo 块任务 + 1 个 GFM 复选框
  });
});
