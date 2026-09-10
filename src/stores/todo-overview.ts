// 任务总览（主控台页面）：跨便签聚合所有 todo 块任务，写操作透传后端命令，成功后回拉刷新。
import { ref } from "vue";
import { defineStore } from "pinia";
import { invoke } from "../composables/useTauri";
import type { TodoBlock, TodoBlockWithSticker, TodoPatch, TodoQueryFilter } from "../types";

export const useTodoOverviewStore = defineStore("todo-overview", () => {
  const blocks = ref<TodoBlockWithSticker[]>([]);
  const loading = ref(false);

  /** 全量拉取（默认不传 filter = 含已完成全部任务）。 */
  async function load(filter?: TodoQueryFilter) {
    loading.value = true;
    try {
      blocks.value = await invoke<TodoBlockWithSticker[]>("list_all_todos_cmd", {
        filter: filter ?? undefined,
      });
    } finally {
      loading.value = false;
    }
  }

  /** 确认收到提醒/逾期（ack）：消除高亮并落库，调度器不再复弹。 */
  async function ack(id: string): Promise<TodoBlock | null> {
    const block = await invoke<TodoBlock | null>("ack_todo_alert_cmd", { id });
    await load();
    return block;
  }

  /** 勾选完成 / 取消完成。 */
  async function toggle(id: string, isCompleted: boolean) {
    await invoke<TodoBlock>("update_todo_block_cmd", { id, patch: { is_completed: isCompleted } });
    await load();
  }

  /** 删除任务（最后一个任务被后端拒绝时上抛错误，由 UI toast 展示）。 */
  async function remove(id: string) {
    await invoke("delete_todo_block_cmd", { id });
    await load();
  }

  /** 在指定 todo 块下新建**父任务**（parentId = 块 id，第 0 层）；返回新任务供 UI 选中编辑。
   *
   * 注意：parentId 必须传块 id——传 null 会被后端当作"新建块"（第 0 层容器），
   * 那会凭空多出一个空 todo 块，而不是在目标块里加任务。
   */
  async function create(stickerId: number, blockId: string): Promise<TodoBlock> {
    const block = await invoke<TodoBlock>("create_todo_block_cmd", {
      stickerId,
      parentId: blockId,
    });
    await load();
    return block;
  }

  /** 局部更新（提醒/截止/重复/标题/描述）。 */
  async function patch(id: string, patch: TodoPatch) {
    await invoke<TodoBlock>("update_todo_block_cmd", { id, patch });
    await load();
  }

  return { blocks, loading, load, ack, toggle, remove, create, patch };
});