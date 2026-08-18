import { nextTick } from "vue";
import { mount } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";

const mathState = vi.hoisted(() => ({ ready: false }));

vi.mock("../../utils/markdown", async () => {
  const { ref } = await import("vue");
  const mathVersion = ref(0);

  return {
    mathVersion,
    renderMarkdown: (content: string) =>
      mathState.ready ? '<span class="math-inline"><svg /></span>' : content,
    collectMathStyle: async () => "",
  };
});

const { default: MarkdownView } = await import("./MarkdownView.vue");
const markdown = await import("../../utils/markdown");

describe("MarkdownView MathJax readiness", () => {
  it("MathJax 异步就绪后重新渲染首次加载的公式", async () => {
    const wrapper = mount(MarkdownView, {
      props: { content: "$E=mc^2$", interactive: false },
    });

    expect(wrapper.html()).toContain("$E=mc^2$");

    mathState.ready = true;
    markdown.mathVersion.value++;
    await nextTick();

    expect(wrapper.find(".math-inline").exists()).toBe(true);
    expect(wrapper.html()).not.toContain("$E=mc^2$");
  });
});
