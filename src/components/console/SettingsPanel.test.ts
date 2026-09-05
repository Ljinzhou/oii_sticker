// SettingsPanel：任务栏隐藏两个开关（默认隐藏 / 编辑模式隐藏）的渲染与切换。
import { describe, it, expect, vi, beforeEach } from "vitest";
import { shallowMount, flushPromises } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import SettingsPanel from "./SettingsPanel.vue";

// vite 注入的构建常量 __APP_VERSION__，测试环境手动补齐
(globalThis as Record<string, unknown>).__APP_VERSION__ = "0.0.0";

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

beforeEach(() => {
  setActivePinia(createPinia());
  mocks.handlers.clear();
  mocks.invokeMock.mockReset();
  mocks.invokeMock.mockImplementation((cmd: string) => {
    switch (cmd) {
      case "get_config_cmd":
        return Promise.resolve({ entries: {} });
      case "autostart_get_cmd":
        return Promise.resolve({
          enabled: false,
          platform: "windows",
          mechanism: "registry",
          launch_args: [],
          executable: "oii_sticker.exe",
        });
      default:
        return Promise.resolve(undefined);
    }
  });
});

async function mountPanel() {
  const wrapper = shallowMount(SettingsPanel, { global: { plugins: [createPinia()] } });
  await flushPromises();
  return wrapper;
}

function rowOf(wrapper: ReturnType<typeof shallowMount>, text: string) {
  const row = wrapper.findAll("label.row").find((r) => r.text().includes(text));
  if (!row) throw new Error(`未找到设置行：${text}`);
  return row;
}

describe("SettingsPanel 任务栏隐藏开关", () => {
  it("通用设置区渲染两个任务栏隐藏开关", async () => {
    const wrapper = await mountPanel();
    const l1 = rowOf(wrapper, "便签隐藏任务栏窗口");
    const l2 = rowOf(wrapper, "编辑模式隐藏任务栏窗口");
    expect(l1.find('input[type="checkbox"]').exists()).toBe(true);
    expect(l2.find('input[type="checkbox"]').exists()).toBe(true);
  });

  it("默认勾选：默认隐藏任务栏=是，编辑模式隐藏=否", async () => {
    const wrapper = await mountPanel();
    const cb1 = rowOf(wrapper, "便签隐藏任务栏窗口").find('input[type="checkbox"]');
    const cb2 = rowOf(wrapper, "编辑模式隐藏任务栏窗口").find('input[type="checkbox"]');
    expect((cb1.element as HTMLInputElement).checked).toBe(true);
    expect((cb2.element as HTMLInputElement).checked).toBe(false);
  });

  it("切换开关写入对应配置键", async () => {
    const wrapper = await mountPanel();
    const cb1 = rowOf(wrapper, "便签隐藏任务栏窗口").find('input[type="checkbox"]');
    const cb2 = rowOf(wrapper, "编辑模式隐藏任务栏窗口").find('input[type="checkbox"]');
    await cb1.setValue(false);
    await cb2.setValue(true);
    expect(mocks.invokeMock).toHaveBeenCalledWith("set_config_cmd", {
      key: "default_sticker_skip_taskbar",
      value: "0",
    });
    expect(mocks.invokeMock).toHaveBeenCalledWith("set_config_cmd", {
      key: "edit_mode_skip_taskbar",
      value: "1",
    });
  });
});