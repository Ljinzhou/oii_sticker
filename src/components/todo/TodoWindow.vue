<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen, invoke } from "../../composables/useTauri";
import { useSettingsStore } from "../../stores/settings";
import { useTodoStore } from "../../stores/todo";
import { reminderToastText, watchTodoReminders } from "../../utils/todo-reminders";
import type { TodoPatch } from "../../types";
import TodoList from "./TodoList.vue";
import TodoDetail from "./TodoDetail.vue";

const todo = useTodoStore(); const settings = useSettingsStore();
const todoId = getCurrentWindow().label.replace(/^todo-/, "");
const upperHeight = ref(220); let stop: (() => void) | undefined;
let stopClose: (() => void) | undefined;
let stopReminder: (() => void) | undefined;
let patchTimer: number | undefined;
let pendingPatch: TodoPatch = {};
const isReady = ref(false);
const loadError = ref<string | null>(null);
const toastMessage = ref<string | null>(null);
let toastTimer: number | undefined;
function showToast(message: string) {
  toastMessage.value = message;
  if (toastTimer) window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => {
    toastMessage.value = null;
  }, 2500);
}
const selected = computed(() => todo.selected);
// 窗口对应的块（独立于当前选中的任务），顶部标题输入框绑定其 block_title
const windowBlock = computed(() => todo.blocks.find((block) => block.id === todoId) ?? null);
const blockTitle = ref("");
let blockTitleDirty = false;
let blockTitleTimer: number | undefined;
watch(windowBlock, (block) => {
  if (!blockTitleDirty) blockTitle.value = block?.block_title ?? "";
}, { immediate: true });
function inputBlockTitle() {
  const block = windowBlock.value;
  if (!block) return;
  blockTitleDirty = true;
  if (blockTitleTimer) window.clearTimeout(blockTitleTimer);
  blockTitleTimer = window.setTimeout(() => {
    blockTitleTimer = undefined;
    if (!blockTitleDirty) return;
    blockTitleDirty = false;
    void todo.update(block.id, { block_title: blockTitle.value }).catch((error) => {
      console.error("[todo] 更新块标题失败：", error);
    });
  }, 250);
}
async function notifyTodoPresence(present: boolean) {
  const sticker = todo.stickerId ?? todo.blocks[0]?.sticker_id;
  if (sticker === null || sticker === undefined) return;
  try {
    await invoke("notify_todo_presence_cmd", { stickerId: sticker, present });
  } catch (error) {
    console.error("[todo] 通报编辑状态失败：", error);
  }
}
async function closeWindow() {
  await notifyTodoPresence(false);
  await invoke("close_todo_window_cmd", { id: todoId });
}
async function handleRemove(id: string) {
  try {
    await todo.remove(id);
    toastMessage.value = null;
  } catch (error) {
    showToast(messageOf(error));
  }
}
async function handleReorder(ids: string[]) {
  try {
    await todo.reorder(ids);
    toastMessage.value = null;
  } catch (error) {
    showToast(messageOf(error));
  }
}
/** 点击高亮任务行 = 确认收到提醒：清除高亮并落库，下次启动不复弹。 */
async function handleAck(id: string) {
  try {
    await invoke("ack_todo_alert_cmd", { id });
    showToast("已确认提醒");
  } catch (error) {
    console.error("[todo] 确认提醒失败：", error);
    showToast(messageOf(error));
  }
}
async function startDragging(event: MouseEvent) { if (event.button !== 0) return; event.preventDefault(); try { await getCurrentWindow().startDragging(); } catch (error) { console.error("[todo] 启动窗口拖动失败：", error); } }

/**
 * 当前块的子树：只保留「本块的父任务」+「这些父任务的子任务」。
 *
 * `todo.blocks` 是全便签的所有块与任务，直接渲染会把别的块也列进来；
 * Todo 窗口是按块打开的（todoId = 块 id），所以这里只取本块的子树。
 */
const blockTasks = computed(() => {
  const direct = todo.blocks.filter((b) => b.parent_id === todoId);
  const parentIds = new Set(direct.map((b) => b.id));
  const subs = todo.blocks.filter((b) => b.parent_id && parentIds.has(b.parent_id));
  return [...direct, ...subs];
});

/**
 * "+ 新建任务"：始终在**当前块**下新建一条**父任务**。
 * 块本身由编辑器 `/` 菜单创建，这里绝不建块（因此不会多出卡片）。
 * 子任务不需要 markdown 标签——块根上的标签已覆盖整棵子树。
 */
async function createRoot() {
  if (!todo.stickerId && todo.blocks.length === 0) return;
  const created = await todo.create(todoId);
  if (created) todo.selectedId = created.id;
}

/**
 * "添加子任务"：只能挂在**父任务**下。
 * 子任务（第 2 层）下不允许再挂，后端也会拒绝。
 */
async function createChild(id?: string) {
  const parentId = id ?? selected.value?.id;
  if (!parentId) return;
  // 无参调用（详情面板）：只有选中「父任务」时才允许添加子任务
  if (!id && selected.value?.parent_id !== todoId) return;
  // 显式 id：只允许挂在父任务上（parent_id 必须等于当前块 id）
  if (id) {
    const target = todo.blocks.find((b) => b.id === id);
    if (!target || target.parent_id !== todoId) return;
  }
  const created = await todo.create(parentId);
  // 保持父任务选中，便于连续添加子任务
  if (created) todo.selectedId = parentId;
}
function beginResize(event: MouseEvent) { const startY = event.clientY; const startHeight = upperHeight.value; const move = (e: MouseEvent) => { upperHeight.value = Math.max(120, Math.min(420, startHeight + e.clientY - startY)); }; const up = () => { document.removeEventListener("mousemove", move); document.removeEventListener("mouseup", up); document.body.style.userSelect = ""; }; document.body.style.userSelect = "none"; document.addEventListener("mousemove", move); document.addEventListener("mouseup", up); }
async function flushSelected() { if (!selected.value || !Object.keys(pendingPatch).length) return; const patch = pendingPatch; pendingPatch = {}; if (patchTimer) { window.clearTimeout(patchTimer); patchTimer = undefined; } await todo.update(selected.value.id, patch); }
function patchSelected(patch: TodoPatch) { if (!selected.value) return; pendingPatch = { ...pendingPatch, ...patch }; if (patchTimer) window.clearTimeout(patchTimer); patchTimer = window.setTimeout(() => { void flushSelected(); }, 250); }
async function saveSelected() {
  await flushSelected();
  if (selected.value) {
    const target = selected.value;
    await todo.update(target.id, {});
    // 通知所属便签窗口展示「{标题} 保存成功」应用内提示，然后自动关闭本窗口。
    try {
      await invoke("notify_todo_saved_cmd", {
        stickerId: windowBlock.value?.sticker_id ?? todo.stickerId,
        title: blockTitle.value.trim() || target.title || "未命名任务",
      });
    } catch (error) {
      console.error("[todo] 通知保存成功失败：", error);
    }
    await closeWindow();
  }
}
function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
async function initialize() {
  try {
    const loaded = await todo.loadForTodo(todoId, true);
    if (!loaded) throw new Error("该 Todo 已不存在或已被删除。");
    await settings.refresh();
    isReady.value = true;
    await notifyTodoPresence(true);
    console.info("[todo-window] Todo 窗口已加载：", todoId);
  } catch (error) {
    loadError.value = messageOf(error);
    console.error("[todo-window] Todo 窗口加载失败：", error);
    return;
  }
  try {
    stop = await listen<string>("todo://updated", () => { void todo.loadForTodo(todoId); });
    stopClose = await getCurrentWindow().onCloseRequested((event) => { event.preventDefault(); void closeWindow(); });
    // 提醒触发：只响应本窗口所属便签的任务，刷新列表 + 弹提示
    stopReminder = await watchTodoReminders({
      onFire: (payload) => {
        if (todo.stickerId !== null && payload.sticker_id !== todo.stickerId) return;
        void todo.loadForTodo(todoId);
        showToast(reminderToastText(payload));
      },
    });
  } catch (error) {
    console.error("[todo-window] Todo 窗口事件监听失败：", error);
  }
}
onMounted(() => { void initialize(); });
onBeforeUnmount(() => { stop?.(); stopClose?.(); stopReminder?.(); if (blockTitleTimer) window.clearTimeout(blockTitleTimer); void notifyTodoPresence(false); void flushSelected(); });
</script>

<template>
  <main class="todo-window"><div class="drag-bar" @mousedown="startDragging"><input v-if="isReady && windowBlock" class="block-title" v-model="blockTitle" placeholder="点击输入任务块标题" @mousedown.stop @click.stop @input="inputBlockTitle" /><button title="关闭" @mousedown.stop @click.stop="closeWindow"><i class="ri-close-line"></i></button></div><p v-if="loadError" class="todo-status todo-error" role="alert">Todo 加载失败：{{ loadError }}</p><p v-else-if="!isReady" class="todo-status">正在加载 Todo...</p><template v-else><TodoList :items="blockTasks" :block-id="todoId" :selected-id="todo.selectedId" :height="upperHeight" @select="todo.selectedId = $event" @ack="handleAck" @create-root="createRoot" @create-child="createChild" @toggle="todo.toggle" @remove="handleRemove" @reorder="handleReorder" /><div class="splitter" @mousedown="beginResize"><i></i></div><TodoDetail :item="selected" :presets="settings.todoPresetConfig" :block-id="todoId" @patch="patchSelected" @create-child="createChild" /><footer><button @click="saveSelected">保存</button></footer><transition name="toast"><p v-if="toastMessage" class="todo-toast" role="alert">{{ toastMessage }}</p></transition></template></main>
</template>

<style scoped>
.todo-window { height:100vh; display:flex; flex-direction:column; overflow:hidden; background:rgba(255,244,214,.95); color:#222; font-family:system-ui,"Microsoft YaHei","PingFang SC",sans-serif; }.drag-bar { height:30px; flex:none; position:relative; cursor:grab; }.block-title { position:absolute; left:10px; top:5px; width:min(52%, 260px); height:20px; box-sizing:border-box; border:1px solid transparent; border-radius:5px; background:rgba(255,255,255,.35); color:#333; font-size:12px; font-family:inherit; padding:0 7px; outline:none; }.block-title:hover { border-color:rgba(0,0,0,.18); background:#fff; }.block-title:focus { border-color:#4f7cff; background:#fff; }.block-title::placeholder { color:#9a9a9a; }.drag-bar button { position:absolute; right:7px; top:4px; border:0; border-radius:5px; background:transparent; color:#777; font-size:20px; line-height:21px; cursor:pointer; }.drag-bar button:hover { color:#d33; background:#ffe3e3; }.todo-status { flex:1; display:grid; place-items:center; margin:0; padding:24px; text-align:center; color:#666; font-size:13px; }.todo-error { color:#b42318; background:rgba(255,255,255,.62); }
.todo-toast { position:fixed; left:50%; bottom:56px; transform:translateX(-50%); max-width:86%; margin:0; padding:8px 14px; background:rgba(180,35,24,.94); color:#fff; font-size:12px; line-height:1.4; border-radius:8px; box-shadow:0 4px 14px rgba(0,0,0,.18); pointer-events:none; z-index:30; }
.toast-enter-active,.toast-leave-active { transition:opacity .2s ease, transform .2s ease; }
.toast-enter-from,.toast-leave-to { opacity:0; transform:translateX(-50%) translateY(8px); }.splitter { height:6px; flex:none; display:flex; align-items:center; justify-content:center; background:rgba(0,0,0,.05); cursor:row-resize; }.splitter i { width:28px; height:2px; border-radius:2px; background:rgba(0,0,0,.2); } footer { flex:none; border-top:1px solid rgba(0,0,0,.08); padding:10px; text-align:center; background:rgba(255,255,255,.9); } footer button { border:0; border-radius:8px; min-width:104px; padding:7px 18px; background:#4f7cff; color:#fff; font-size:13px; font-family:inherit; cursor:pointer; } footer button:hover { background:#3b67e8; }
</style>
