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

    // ── 分组（= 文件夹：层级 / 颜色 / 排序） ──
    /** 新建分组；`parentId` 为 null 表示顶层。 */
    async createGroup(name: string, parentId: number | null = null): Promise<StickerGroup> {
      const group = await invoke<StickerGroup>("group_create_cmd", { name, parentId });
      await this.refresh();
      return group;
    },
    async renameGroup(id: number, name: string) {
      await invoke("group_rename_cmd", { id, name });
      await this.refresh();
    },
    /** 设置分组颜色；null = 清除颜色。 */
    async setGroupColor(id: number, color: string | null) {
      await invoke("group_set_color_cmd", { id, color });
      await this.refresh();
    },
    /** 同级重排：`ids` 为该父级下的目标顺序（其它层级自动忽略）。 */
    async reorderGroups(parentId: number | null, ids: number[]) {
      await invoke("group_reorder_cmd", { parentId, ids });
      await this.refresh();
    },
    /** 移动分组到新父级（null = 顶层）；后端拒绝放入自身或自己的子树。 */
    async moveGroup(id: number, parentId: number | null) {
      await invoke("group_move_cmd", { id, parentId });
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

    // ── 文件系统（在资源管理器中查看） ──
    /** 在资源管理器中定位该便签的 md 文件（选中文件）；返回文件完整路径。 */
    async openStickerInExplorer(id: number): Promise<string> {
      return await invoke<string>("open_sticker_in_explorer_cmd", { id });
    },
    /** 在资源管理器中打开分组文件夹（`null` = stickers 根目录）；返回被打开的目录。 */
    async openGroupInExplorer(id: number | null): Promise<string> {
      return await invoke<string>("open_group_in_explorer_cmd", { id });
    },

    // ── 分组树工具（纯函数；树状 UI 与平铺分行共用） ──
    /** 某父级下的直接子分组（同级按 sort_order, id 排序）。 */
    childrenOf(parentId: number | null): StickerGroup[] {
      return this.groups
        .filter((g) => (g.parent_id ?? null) === parentId)
        .sort((a, b) => a.sort_order - b.sort_order || a.id - b.id);
    },
    /** 分组的完整路径（如「学习 / 英语」）。 */
    groupPath(id: number): string {
      const byId = new Map(this.groups.map((g) => [g.id, g]));
      const parts: string[] = [];
      let cur = byId.get(id);
      let guard = 0;
      while (cur && guard++ < 64) {
        parts.unshift(cur.name);
        cur = cur.parent_id != null ? byId.get(cur.parent_id) : undefined;
      }
      return parts.join(" / ");
    },
    /** 该分组及其所有后代分组的 id 集合。 */
    groupAndDescendants(id: number): Set<number> {
      const ids = new Set<number>([id]);
      let grew = true;
      while (grew) {
        grew = false;
        for (const g of this.groups) {
          if (g.parent_id != null && ids.has(g.parent_id) && !ids.has(g.id)) {
            ids.add(g.id);
            grew = true;
          }
        }
      }
      return ids;
    },
    /** 分组（含子分组）内的便签；`null` = 未分组。 */
    stickersInGroup(id: number | null): Sticker[] {
      if (id === null) return this.stickers.filter((s) => s.group_id == null);
      const ids = this.groupAndDescendants(id);
      return this.stickers.filter((s) => s.group_id != null && ids.has(s.group_id));
    },
    /** 顶层分组（平铺视图按文件夹分行时使用）。 */
    topLevelGroups(): StickerGroup[] {
      return this.childrenOf(null);
    },
  },
});
