// 系统设置 store（system_config 快照）
import { defineStore } from "pinia";
import { invoke } from "../composables/useTauri";
import { DEFAULT_PRESETS, type PresetItem, type PresetKind } from "../utils/presets";
import type { SystemConfig } from "../types";

/** 解析配置中的预设 JSON，非法/缺失回退出厂默认。 */
function parsePresets(key: string, fallback: PresetItem[]): PresetItem[] {
  try {
    const value = JSON.parse(key);
    return Array.isArray(value) ? value : fallback;
  } catch {
    return fallback;
  }
}

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
    editFontFamily: (state) =>
      state.config.entries["edit_font_family"] ?? "Microsoft YaHei",
    // 交互模式无操作后自动收起回展示模式的秒数（系统设置可改，默认 5）
    autoCollapseSecs: (state) =>
      parseInt(state.config.entries["auto_collapse_secs"] ?? "5", 10),
    autoScrollSpeed: (state) =>
      parseInt(state.config.entries["default_sticker_auto_scroll_speed"] ?? "30", 10),
    recentSlashCommands: (state): string[] => {
      try {
        const value = JSON.parse(state.config.entries.recent_slash_commands ?? "[]");
        return Array.isArray(value) ? value.filter((item): item is string => typeof item === "string").slice(0, 5) : [];
      } catch {
        return [];
      }
    },
    // Todo 预设（提醒/截止/重复三组），缺省用出厂内置（未落库）
    todoPresets: (state) => ({
      reminders: parsePresets(state.config.entries.todo_preset_reminders ?? "null", DEFAULT_PRESETS.reminders),
      due: parsePresets(state.config.entries.todo_preset_due ?? "null", DEFAULT_PRESETS.due),
      repeats: parsePresets(state.config.entries.todo_preset_repeats ?? "null", DEFAULT_PRESETS.repeats),
    }),
  },
  actions: {
    async refresh() {
      const cfg = await invoke<SystemConfig>("get_config_cmd");
      // 防御：后端异常/非法返回时不破坏 store 快照
      this.config = cfg && typeof cfg === "object" && cfg.entries ? cfg : { entries: {} } as SystemConfig;
    },
    async set(key: string, value: string) {
      await invoke("set_config_cmd", { key, value });
      this.config.entries[key] = value;
    },
    /** 保存一组 Todo 预设（设置页增删改后调用）。 */
    async setTodoPresets(kind: PresetKind, items: PresetItem[]) {
      const key = kind === "reminders" ? "todo_preset_reminders" : kind === "due" ? "todo_preset_due" : "todo_preset_repeats";
      await this.set(key, JSON.stringify(items));
    },
  },
});
