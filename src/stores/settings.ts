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
    bgColor: (state) => state.config.entries["default_sticker_bg_color"] ?? "#CCFFCC",
    titleFontSize: (state) =>
      parseInt(state.config.entries["default_sticker_title_font_size"] ?? "14", 10),
    bodyFontSize: (state) =>
      parseInt(state.config.entries["default_sticker_body_font_size"] ?? "13", 10),
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
