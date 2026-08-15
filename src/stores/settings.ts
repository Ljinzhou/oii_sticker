// 系统设置 store（system_config 快照）
import { defineStore } from "pinia";
import { invoke } from "../composables/useTauri";
import type { SystemConfig } from "../types";

export const useSettingsStore = defineStore("settings", {
  state: () => ({
    config: { entries: {} } as SystemConfig,
  }),
  getters: {
    get: (state) => (key: string, fallback = "") =>
      state.config.entries[key] ?? fallback,
    opacity: (state) => parseFloat(state.config.entries["default_sticker_opacity"] ?? "0.9"),
    bgColor: (state) => state.config.entries["default_sticker_bg_color"] ?? "#FFF4D6",
    bodyFontSize: (state) =>
      parseInt(state.config.entries["default_sticker_body_font_size"] ?? "13", 10),
    // 交互模式无操作后自动收起回展示模式的秒数（系统设置可改，默认 5）
    autoCollapseSecs: (state) =>
      parseInt(state.config.entries["auto_collapse_secs"] ?? "5", 10),
    autoScrollSpeed: (state) =>
      parseInt(state.config.entries["default_sticker_auto_scroll_speed"] ?? "30", 10),
  },
  actions: {
    async refresh() {
      this.config = await invoke<SystemConfig>("get_config_cmd");
    },
    async set(key: string, value: string) {
      await invoke("set_config_cmd", { key, value });
      this.config.entries[key] = value;
    },
  },
});
