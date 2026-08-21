import { describe, it, expect } from "vitest";
import { mount } from "@vue/test-utils";
import MarkdownView from "./MarkdownView.vue";
import markdownViewSource from "./MarkdownView.vue?raw";

describe("MarkdownView", () => {
  it("渲染内容为 HTML（v-html）", () => {
    const wrapper = mount(MarkdownView, {
      props: { content: "# 标题", interactive: false },
    });
    expect(wrapper.html()).toContain("<h1>标题</h1>");
  });

  it("点击任务 checkbox 发出 toggle（源行号）", async () => {
    const wrapper = mount(MarkdownView, {
      props: { content: "- [ ] 待办", interactive: true },
    });
    const checkbox = wrapper.find("input.task-checkbox");
    expect(checkbox.exists()).toBe(true);
    expect(checkbox.attributes("data-line")).toBe("0");

    await checkbox.trigger("click");
    const emitted = wrapper.emitted("toggle");
    expect(emitted).toBeTruthy();
    expect(emitted![0]).toEqual([0]);
  });

  it("非交互模式点击 checkbox 不发出 toggle", async () => {
    const wrapper = mount(MarkdownView, {
      props: { content: "- [ ] 待办", interactive: false },
    });
    await wrapper.find("input.task-checkbox").trigger("click");
    expect(wrapper.emitted("toggle")).toBeUndefined();
  });

  it("有序列表编号与无序列表对齐且使用正文颜色", () => {
    const orderedMarkerCss = markdownViewSource.match(
      /\.markdown :deep\(ol > li::before\)\s*\{([^}]*)\}/,
    )?.[1];

    expect(orderedMarkerCss).toBeTruthy();
    expect(orderedMarkerCss).toMatch(/margin-left:\s*-22px/);
    expect(orderedMarkerCss).toMatch(/color:\s*inherit/);
  });

  it("Todo 卡片在交互态可切换，卡片空白处打开窗口", async () => {
    const todo = { id: "t-1", sticker_id: 1, title: "购物", block_title: "", description: null, is_completed: false, parent_id: null, reminder_at: null, due_at: null, repeat_rule: null, created_at: "", updated_at: "" };
    const wrapper = mount(MarkdownView, { props: { content: '<todo-block id="t-1"></todo-block>', interactive: true, todoBlocks: [todo] } });
    await wrapper.find(".todo-task-checkbox").trigger("click");
    expect(wrapper.emitted("toggleTodo")?.[0]).toEqual(["t-1", true]);
    await wrapper.find(".tb-head").trigger("click");
    expect(wrapper.emitted("openTodo")?.[0]).toEqual(["t-1"]);
  });
});
