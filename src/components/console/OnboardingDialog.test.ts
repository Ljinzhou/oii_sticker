import { describe, it, expect, vi, beforeEach } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";

const mocks = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("../../composables/useTauri", () => ({
  invoke: (...args: unknown[]) => mocks.invokeMock(...args),
}));

import OnboardingDialog from "./OnboardingDialog.vue";

async function mountDialog() {
  const wrapper = mount(OnboardingDialog);
  await flushPromises();
  return wrapper;
}

beforeEach(() => {
  mocks.invokeMock.mockReset();
  mocks.invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "workspace_default_path_cmd") {
      return Promise.resolve("C:/Users/demo/Documents/oiistiker_workspace");
    }
    return Promise.resolve(undefined);
  });
});

describe("OnboardingDialog", () => {
  it("挂载时从 workspace_default_path_cmd 填入默认路径", async () => {
    const wrapper = await mountDialog();
    const input = wrapper.get<HTMLInputElement>("input.onboarding-path");
    expect(input.element.value).toBe("C:/Users/demo/Documents/oiistiker_workspace");
  });

  it("空路径点击确认显示错误，不调用 bootstrap", async () => {
    const wrapper = await mountDialog();
    mocks.invokeMock.mockClear();
    await wrapper.get<HTMLInputElement>("input.onboarding-path").setValue("   ");
    await wrapper.get(".onboarding-confirm").trigger("click");
    await flushPromises();
    expect(wrapper.find(".errorMsg").exists()).toBe(true);
    expect(wrapper.text()).toContain("请填写工作空间目录");
    expect(
      mocks.invokeMock.mock.calls.some((c) => c[0] === "workspace_bootstrap_cmd"),
    ).toBe(false);
    expect(wrapper.emitted("done")).toBeUndefined();
  });

  it("带路径确认时以 { path, name, removeLegacy } 调用 bootstrap 并 emit done", async () => {
    const wrapper = await mountDialog();
    mocks.invokeMock.mockClear();
    mocks.invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "workspace_default_path_cmd") {
        return Promise.resolve("C:/Users/demo/Documents/oiistiker_workspace");
      }
      if (cmd === "workspace_bootstrap_cmd") {
        return Promise.resolve({ id: "w-1", name: "未命名工作空间" });
      }
      return Promise.resolve(undefined);
    });
    await wrapper.get<HTMLInputElement>("input.onboarding-path").setValue(" C:/ws ");
    await wrapper.get(".onboarding-confirm").trigger("click");
    await flushPromises();
    expect(mocks.invokeMock).toHaveBeenCalledWith("workspace_bootstrap_cmd", {
      path: "C:/ws",
      name: "未命名工作空间",
      removeLegacy: false,
    });
    expect(wrapper.emitted("done")).toHaveLength(1);
  });

  it("bootstrap 失败时显示错误且不 emit done", async () => {
    const wrapper = await mountDialog();
    mocks.invokeMock.mockClear();
    mocks.invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "workspace_default_path_cmd") {
        return Promise.resolve("C:/Users/demo/Documents/oiistiker_workspace");
      }
      if (cmd === "workspace_bootstrap_cmd") {
        return Promise.reject("目录创建失败：权限不足");
      }
      return Promise.resolve(undefined);
    });
    await wrapper.get<HTMLInputElement>("input.onboarding-path").setValue("C:/ws");
    await wrapper.get(".onboarding-confirm").trigger("click");
    await flushPromises();
    expect(wrapper.find(".errorMsg").exists()).toBe(true);
    expect(wrapper.text()).toContain("目录创建失败：权限不足");
    expect(wrapper.emitted("done")).toBeUndefined();
  });
});
