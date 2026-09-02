import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { invoke } from "../composables/useTauri";
import type { TodoBlock, TodoPatch } from "../types";

export const useTodoStore = defineStore("todo", () => {
  const blocks = ref<TodoBlock[]>([]);
  const selectedId = ref<string | null>(null);
  const stickerId = ref<number | null>(null);
  const loading = ref(false);
  const selected = computed(() => blocks.value.find((block) => block.id === selectedId.value) ?? null);

  function replace(block: TodoBlock) {
    const index = blocks.value.findIndex((item) => item.id === block.id);
    if (index >= 0) blocks.value.splice(index, 1, block);
    else blocks.value.push(block);
  }

  async function loadForSticker(nextStickerId: number, preferredId?: string) {
    loading.value = true;
    try {
      stickerId.value = nextStickerId;
      blocks.value = await invoke<TodoBlock[]>("list_todo_for_sticker_cmd", { stickerId: nextStickerId });
      const wanted = preferredId ?? selectedId.value;
      selectedId.value = blocks.value.some((block) => block.id === wanted)
        ? wanted!
        : blocks.value.find((block) => !block.parent_id)?.id ?? blocks.value[0]?.id ?? null;
    } finally {
      loading.value = false;
    }
  }

  /** 加载窗口所辖块。selectBlock=true 仅在打开窗口时使用；
   *  后续 todo://updated 刷新必须保持用户当前选中（否则编辑任务时
   *  补丁回写会把选中强制切回窗口根任务，导致输入焦点跳失）。 */
  async function loadForTodo(todoId: string, selectBlock = false) {
    const block = await invoke<TodoBlock | null>("get_todo_block_cmd", { id: todoId });
    if (!block) {
      // 窗口所辖块已被删除：不得清空整个便签的任务列表，
      // 按已知 sticker 全量刷新（其余块仍可编辑），仅剔除该块关联的选中项。
      if (stickerId.value !== null) {
        await loadForSticker(stickerId.value);
      } else {
        blocks.value = blocks.value.filter((item) => item.id !== todoId && item.parent_id !== todoId);
        if (selectedId.value === todoId) selectedId.value = blocks.value[0]?.id ?? null;
      }
      return null;
    }
    await loadForSticker(block.sticker_id, selectBlock ? todoId : undefined);
    return block;
  }

  async function create(parentId?: string) {
    if (stickerId.value === null) return null;
    // 显式 null 而不是 undefined：与 Rust `Option<String>` 的"无父任务"语义一致，
    // 避免 invoke JSON 序列化时把 undefined 字段直接丢掉（导致 mock 难以断言）。
    const block = await invoke<TodoBlock>("create_todo_block_cmd", {
      stickerId: stickerId.value,
      parentId: parentId ?? null,
    });
    replace(block);
    selectedId.value = block.id;
    return block;
  }

  async function update(id: string, patch: TodoPatch) {
    const block = await invoke<TodoBlock>("update_todo_block_cmd", { id, patch });
    replace(block);
    return block;
  }

  async function remove(id: string) {
    await invoke("delete_todo_block_cmd", { id });
    const doomed = new Set([id]);
    let grew = true;
    while (grew) {
      grew = false;
      for (const block of blocks.value) {
        if (block.parent_id && doomed.has(block.parent_id) && !doomed.has(block.id)) {
          doomed.add(block.id);
          grew = true;
        }
      }
    }
    blocks.value = blocks.value.filter((block) => !doomed.has(block.id));
    if (selectedId.value && doomed.has(selectedId.value)) selectedId.value = blocks.value[0]?.id ?? null;
  }

  async function toggle(id: string, isCompleted: boolean) {
    return update(id, { is_completed: isCompleted });
  }

  /** 拖拽排序：ids 为同一分组（根或同父子任务）的完整新顺序。乐观更新，失败回滚。 */
  async function reorder(ids: string[]) {
    if (ids.length < 2) return;
    const groupId = blocks.value.find((block) => block.id === ids[0])?.parent_id ?? null;
    const rank = new Map(ids.map((id, index) => [id, index]));
    blocks.value = blocks.value.slice().sort((a, b) => {
      const inA = a.parent_id === groupId;
      const inB = b.parent_id === groupId;
      if (inA && inB) return (rank.get(a.id) ?? Number.MAX_SAFE_INTEGER) - (rank.get(b.id) ?? Number.MAX_SAFE_INTEGER);
      return 0;
    });
    try {
      await invoke("reorder_todo_cmd", { ids });
    } catch (error) {
      // 还原为服务端顺序并上抛（UI 展示错误 toast）
      if (stickerId.value !== null) await loadForSticker(stickerId.value);
      throw error;
    }
  }

  return { blocks, selectedId, stickerId, loading, selected, loadForSticker, loadForTodo, create, update, remove, toggle, reorder };
});
