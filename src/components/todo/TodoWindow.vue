<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen, invoke } from "../../composables/useTauri";
import { useSettingsStore } from "../../stores/settings";
import { useTodoStore } from "../../stores/todo";
import type { TodoPatch } from "../../types";
import TodoList from "./TodoList.vue";
import TodoDetail from "./TodoDetail.vue";

const todo = useTodoStore(); const settings = useSettingsStore();
const todoId = getCurrentWindow().label.replace(/^todo-/, "");
const upperHeight = ref(220); let stop: (() => void) | undefined;
let stopClose: (() => void) | undefined;
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
async function startDragging(event: MouseEvent) { if (event.button !== 0) return; event.preventDefault(); try { await getCurrentWindow().startDragging(); } catch (error) { console.error("[todo] 启动窗口拖动失败：", error); } }
async function createRoot() {
  const block = await todo.create();
  if (block) {
    try {
      // 根任务在正文中补一个 <todo-block> 标记，避免块与文档脱节（孤儿）
      await invoke("sync_todo_marker_cmd", { stickerId: block.sticker_id, id: block.id });
    } catch (error) {
      console.error("[todo] 同步 Todo 标记失败：", error);
    }
  }
}
async function createChild(id?: string) {
  // 显式 id（列表"添加子任务"行）不受选中项限制；无参调用（详情面板）时选中子任务不创建
  const parentId = id ?? selected.value?.id;
  if (!parentId) return;
  if (!id && selected.value?.parent_id) return;
  const block = await todo.create(parentId);
  // 保持父根选中，便于连续添加子任务
  if (block) todo.selectedId = parentId;
}
function beginResize(event: MouseEvent) { const startY = event.clientY; const startHeight = upperHeight.value; const move = (e: MouseEvent) => { upperHeight.value = Math.max(120, Math.min(420, startHeight + e.clientY - startY)); }; const up = () => { document.removeEventListener("mousemove", move); document.removeEventListener("mouseup", up); document.body.style.userSelect = ""; }; document.body.style.userSelect = "none"; document.addEventListener("mousemove", move); document.addEventListener("mouseup", up); }
async function flushSelected() { if (!selected.value || !Object.keys(pendingPatch).length) return; const patch = pendingPatch; pendingPatch = {}; if (patchTimer) { window.clearTimeout(patchTimer); patchTimer = undefined; } await todo.update(selected.value.id, patch); }
function patchSelected(patch: TodoPatch) { if (!selected.value) return; pendingPatch = { ...pendingPatch, ...patch }; if (patchTimer) window.clearTimeout(patchTimer); patchTimer = window.setTimeout(() => { void flushSelected(); }, 250); }
async function saveSelected() { await flushSelected(); if (selected.value) await todo.update(selected.value.id, {}); }
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
  } catch (error) {
    console.error("[todo-window] Todo 窗口事件监听失败：", error);
  }
}
onMounted(() => { void initialize(); });
onBeforeUnmount(() => { stop?.(); stopClose?.(); if (blockTitleTimer) window.clearTimeout(blockTitleTimer); void notifyTodoPresence(false); void flushSelected(); });
</script>

<template>
  <main class="todo-window"><div class="drag-bar" @mousedown="startDragging"><input v-if="isReady && windowBlock" class="block-title" v-model="blockTitle" placeholder="输入块标题" @mousedown.stop @click.stop @input="inputBlockTitle" /><button title="关闭" @mousedown.stop @click.stop="closeWindow"><i class="ri-close-line"></i></button></div><p v-if="loadError" class="todo-status todo-error" role="alert">Todo 加载失败：{{ loadError }}</p><p v-else-if="!isReady" class="todo-status">正在加载 Todo...</p><template v-else><TodoList :items="todo.blocks" :selected-id="todo.selectedId" :height="upperHeight" @select="todo.selectedId = $event" @create-root="createRoot" @create-child="createChild" @toggle="todo.toggle" @remove="handleRemove" @reorder="handleReorder" /><div class="splitter" @mousedown="beginResize"><i></i></div><TodoDetail :item="selected" :presets="settings.todoPresetConfig" @patch="patchSelected" @create-child="createChild" /><footer><button @click="saveSelected">保存</button></footer><transition name="toast"><p v-if="toastMessage" class="todo-toast" role="alert">{{ toastMessage }}</p></transition></template></main>
</template>

<style scoped>
.todo-window { height:100vh; display:flex; flex-direction:column; overflow:hidden; background:rgba(255,244,214,.95); color:#222; font-family:system-ui,"Microsoft YaHei","PingFang SC",sans-serif; }.drag-bar { height:30px; flex:none; position:relative; cursor:grab; }.block-title { position:absolute; left:10px; top:5px; width:min(52%, 260px); height:20px; box-sizing:border-box; border:1px solid transparent; border-radius:5px; background:rgba(255,255,255,.35); color:#333; font:12px inherit; padding:0 7px; outline:none; }.block-title:hover { border-color:rgba(0,0,0,.18); background:#fff; }.block-title:focus { border-color:#4f7cff; background:#fff; }.block-title::placeholder { color:#9a9a9a; }.drag-bar button { position:absolute; right:7px; top:4px; border:0; border-radius:5px; background:transparent; color:#777; font-size:20px; line-height:21px; cursor:pointer; }.drag-bar button:hover { color:#d33; background:#ffe3e3; }.todo-status { flex:1; display:grid; place-items:center; margin:0; padding:24px; text-align:center; color:#666; font-size:13px; }.todo-error { color:#b42318; background:rgba(255,255,255,.62); }
.todo-toast { position:fixed; left:50%; bottom:56px; transform:translateX(-50%); max-width:86%; margin:0; padding:8px 14px; background:rgba(180,35,24,.94); color:#fff; font-size:12px; line-height:1.4; border-radius:8px; box-shadow:0 4px 14px rgba(0,0,0,.18); pointer-events:none; z-index:30; }
.toast-enter-active,.toast-leave-active { transition:opacity .2s ease, transform .2s ease; }
.toast-enter-from,.toast-leave-to { opacity:0; transform:translateX(-50%) translateY(8px); }.splitter { height:6px; flex:none; display:flex; align-items:center; justify-content:center; background:rgba(0,0,0,.05); cursor:row-resize; }.splitter i { width:28px; height:2px; border-radius:2px; background:rgba(0,0,0,.2); } footer { flex:none; border-top:1px solid rgba(0,0,0,.08); padding:10px; text-align:center; background:rgba(255,255,255,.9); } footer button { border:0; border-radius:8px; min-width:104px; padding:7px 18px; background:#4f7cff; color:#fff; font:13px inherit; cursor:pointer; } footer button:hover { background:#3b67e8; }
</style>
