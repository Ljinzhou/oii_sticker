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

const sticker = {
  id: 1,
  parent_id: null,
  title: "欢迎使用 oii_sticker",
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

function setupInvoke() {
  mocks.invokeMock.mockImplementation((cmd: string) => {
    switch (cmd) {
      case "list_stickers_cmd":
        return Promise.resolve([sticker]);
      case "list_open_sticker_ids_cmd":
        return Promise.resolve([1]);
      case "get_config_cmd":
        return Promise.resolve({ entries: {} });
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
  setupInvoke();
});

describe("ConsoleView", () => {
  it("卡片不渲染模式徽章（无 display_mode 文本标签）", async () => {
    const wrapper = await mountConsole();
    expect(wrapper.find(".mode-badge").exists()).toBe(false);
    expect(wrapper.text()).not.toContain("edit");
  });
});
