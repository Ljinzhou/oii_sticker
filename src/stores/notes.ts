// 便签列表 store（主控台单一真相源）
import { defineStore } from "pinia";
import { invoke } from "../composables/useTauri";
import type { NewSticker, Sticker, StickerPatch } from "../types";

export const useNotesStore = defineStore("notes", {
  state: () => ({
    stickers: [] as Sticker[],
    loading: false,
  }),
  actions: {
    async refresh() {
      this.loading = true;
      try {
        this.stickers = await invoke<Sticker[]>("list_stickers_cmd");
      } finally {
        this.loading = false;
      }
    },
    async create(newSticker: NewSticker): Promise<number> {
      const id = await invoke<number>("create_sticker_cmd", { new: newSticker });
      await this.refresh();
      return id;
    },
    async update(id: number, patch: StickerPatch) {
      await invoke("update_sticker_cmd", { id, patch });
      await this.refresh();
    },
    async remove(id: number) {
      await invoke("delete_sticker_cmd", { id });
      await this.refresh();
    },
    getById(id: number): Sticker | undefined {
      return this.stickers.find((s) => s.id === id);
    },
  },
});
