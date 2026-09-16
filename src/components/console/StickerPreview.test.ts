// StickerPreview：统计信息全部在上 → 一条分割线 → 分割线以下全部是正文预览内容。
import { describe, it, expect, beforeEach } from "vitest";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import StickerPreview from "./StickerPreview.vue";
import type { Sticker } from "../../types";

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

beforeEach(() => setActivePinia(createPinia()));

describe("StickerPreview 结构", () => {
  it("统计在上、分割线、分割线以下只有正文预览", () => {
    const wrapper = mount(StickerPreview, { props: { sticker: mkSticker() } });
    const html = wrapper.html();
    const iMetrics = html.indexOf('class="metrics"');
    const iMeta = html.indexOf('class="meta-line"');
    const iBody = html.indexOf('class="pv-body"');
    expect(iMetrics).toBeGreaterThanOrEqual(0);
    expect(iMeta).toBeGreaterThan(iMetrics); // 结构统计在统计卡之后
    expect(iBody).toBeGreaterThan(iMeta); // 分割线（pv-body 上边框）在最后

    const body = wrapper.find(".pv-body");
    expect(body.find(".md-body").exists()).toBe(true);
    expect(body.text()).not.toContain("结构统计");
    expect(body.find(".kv").exists()).toBe(false); // 旧的结构统计列表已移除

    const meta = wrapper.find(".meta-line");
    expect(meta.text()).toContain("标题");
    expect(meta.text()).toContain("8f3a91c2");
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
