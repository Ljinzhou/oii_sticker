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
    /** 本地即时应用偏好（视觉立即生效，不做网络往返）——滑块拖动时调用。 */
    applyLocal(patch: {
      opacity?: number;
      bg_color?: string;
      text_color?: string;
      title_font_size?: number;
      body_font_size?: number;
    }) {
      const e = this.effective;
      if (!e) return;
      if (patch.opacity !== undefined) e.opacity = patch.opacity;
      if (patch.bg_color !== undefined) e.bg_color = patch.bg_color;
      if (patch.text_color !== undefined) e.text_color = patch.text_color;
      if (patch.title_font_size !== undefined) e.title_font_size = patch.title_font_size;
      if (patch.body_font_size !== undefined) e.body_font_size = patch.body_font_size;
    },
    /** 持久化到后端（松手/防抖后调用）。 */
    async save(stickerId: number, prefs: Partial<StickerPrefs>) {
      await invoke("update_sticker_prefs_cmd", {
        prefs: { sticker_id: stickerId, ...prefs },
      });
      // 用后端返回结果校准（合并链可能改变最终值）
      await this.load(stickerId);
    },
    async reset(stickerId: number) {
      await invoke("reset_sticker_prefs_cmd", { id: stickerId });
      await this.load(stickerId);
    },
  },
});
