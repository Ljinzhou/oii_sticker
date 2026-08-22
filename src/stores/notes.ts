// 便签列表 store（主控台单一真相源）
import { defineStore } from "pinia";
import { invoke } from "../composables/useTauri";
import type { NewSticker, Sticker, StickerGroup, StickerPatch } from "../types";

export const useNotesStore = defineStore("notes", {
  state: () => ({
    stickers: [] as Sticker[],
    groups: [] as StickerGroup[],
    loading: false,
  }),
  actions: {
    async refresh() {
      this.loading = true;
      try {
        const [stickers, groups] = await Promise.all([
          invoke<Sticker[]>("list_stickers_cmd"),
          invoke<StickerGroup[]>("group_list_cmd"),
        ]);
        this.stickers = stickers;
        this.groups = groups;
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
    async createGroup(name: string): Promise<StickerGroup> {
      return await invoke<StickerGroup>("group_create_cmd", { name });
    },
    async renameGroup(id: number, name: string) {
      await invoke("group_rename_cmd", { id, name });
      await this.refresh();
    },
    async deleteGroup(id: number, mode: "to-default" | "with-stickers"): Promise<number> {
      const removed = await invoke<number>("group_delete_cmd", { id, mode });
      await this.refresh();
      return removed;
    },
    async moveStickerToGroup(stickerId: number, groupId: number | null) {
      await invoke("move_sticker_group_cmd", { stickerId, groupId });
      await this.refresh();
    },
  },
});
