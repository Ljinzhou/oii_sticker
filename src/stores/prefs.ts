// 便签偏好（单窗口使用：prefs + effective 合并结果）
import { defineStore } from "pinia";
import { invoke } from "../composables/useTauri";
import type { EffectivePrefs, StickerPrefs } from "../types";

export const usePrefsStore = defineStore("prefs", {
  state: () => ({
    effective: null as EffectivePrefs | null,
  }),
  actions: {
    async load(stickerId: number) {
      this.effective = await invoke<EffectivePrefs>("effective_prefs_cmd", { id: stickerId });
    },
    async save(stickerId: number, prefs: Partial<StickerPrefs>) {
      await invoke("update_sticker_prefs_cmd", {
        prefs: { sticker_id: stickerId, ...prefs },
      });
      await this.load(stickerId);
    },
    async reset(stickerId: number) {
      await invoke("reset_sticker_prefs_cmd", { id: stickerId });
      await this.load(stickerId);
    },
  },
});
