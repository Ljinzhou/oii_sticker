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

  async function loadForTodo(todoId: string) {
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
    await loadForSticker(block.sticker_id, todoId);
    return block;
  }

  async function create(parentId?: string) {
    if (stickerId.value === null) return null;
    const block = await invoke<TodoBlock>("create_todo_block_cmd", {
      stickerId: stickerId.value,
      parentId,
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

  return { blocks, selectedId, stickerId, loading, selected, loadForSticker, loadForTodo, create, update, remove, toggle };
});
