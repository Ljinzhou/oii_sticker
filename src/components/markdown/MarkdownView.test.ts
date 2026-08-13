import { describe, it, expect } from "vitest";
import { mount } from "@vue/test-utils";
import MarkdownView from "./MarkdownView.vue";

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
});
