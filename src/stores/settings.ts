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
    todoPresetConfig: (state) => ({
      remindTomorrowHour: parseInt(state.config.entries.todo_remind_tomorrow_hour ?? "9", 10),
      remindNextWeekDow: parseInt(state.config.entries.todo_remind_next_week_dow ?? "1", 10),
      remindNextWeekHour: parseInt(state.config.entries.todo_remind_next_week_hour ?? "9", 10),
      dueTodayHour: parseInt(state.config.entries.todo_due_today_hour ?? "18", 10),
      dueTomorrowHour: parseInt(state.config.entries.todo_due_tomorrow_hour ?? "9", 10),
      dueNextWeekDow: parseInt(state.config.entries.todo_due_next_week_dow ?? "1", 10),
    }),
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
