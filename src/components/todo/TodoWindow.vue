<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
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
const selected = computed(() => todo.selected);
async function startDragging(event: MouseEvent) { if (event.button !== 0) return; event.preventDefault(); try { await getCurrentWindow().startDragging(); } catch (error) { console.error("[todo] 启动窗口拖动失败：", error); } }
async function createRoot() { await todo.create(); }
async function createChild(id?: string) { const parentId = id ?? selected.value?.id; if (!parentId || selected.value?.parent_id) return; await todo.create(parentId); }
function beginResize(event: MouseEvent) { const startY = event.clientY; const startHeight = upperHeight.value; const move = (e: MouseEvent) => { upperHeight.value = Math.max(120, Math.min(420, startHeight + e.clientY - startY)); }; const up = () => { document.removeEventListener("mousemove", move); document.removeEventListener("mouseup", up); document.body.style.userSelect = ""; }; document.body.style.userSelect = "none"; document.addEventListener("mousemove", move); document.addEventListener("mouseup", up); }
async function flushSelected() { if (!selected.value || !Object.keys(pendingPatch).length) return; const patch = pendingPatch; pendingPatch = {}; if (patchTimer) { window.clearTimeout(patchTimer); patchTimer = undefined; } await todo.update(selected.value.id, patch); }
function patchSelected(patch: TodoPatch) { if (!selected.value) return; pendingPatch = { ...pendingPatch, ...patch }; if (patchTimer) window.clearTimeout(patchTimer); patchTimer = window.setTimeout(() => { void flushSelected(); }, 250); }
async function saveSelected() { await flushSelected(); if (selected.value) await todo.update(selected.value.id, {}); }
onMounted(async () => { await settings.refresh(); await todo.loadForTodo(todoId); stop = await listen<string>("todo://updated", () => todo.loadForTodo(todoId)); stopClose = await getCurrentWindow().onCloseRequested((event) => { event.preventDefault(); invoke("close_todo_window_cmd", { id: todoId }); }); });
onBeforeUnmount(() => { stop?.(); stopClose?.(); void flushSelected(); });
</script>

<template>
  <main class="todo-window"><div class="drag-bar" @mousedown="startDragging"><button title="关闭" @mousedown.stop @click.stop="invoke('close_todo_window_cmd', { id: todoId })">×</button></div><TodoList :items="todo.blocks" :selected-id="todo.selectedId" :height="upperHeight" @select="todo.selectedId = $event" @create-root="createRoot" @create-child="createChild" @toggle="todo.toggle" /><div class="splitter" @mousedown="beginResize"><i></i></div><TodoDetail :item="selected" :presets="settings.todoPresetConfig" @patch="patchSelected" @create-child="createChild" /><footer><button @click="saveSelected">保存</button></footer></main>
</template>

<style scoped>
.todo-window { height:100vh; display:flex; flex-direction:column; overflow:hidden; background:rgba(255,244,214,.95); color:#222; font-family:system-ui,"Microsoft YaHei","PingFang SC",sans-serif; }.drag-bar { height:30px; flex:none; position:relative; cursor:grab; }.drag-bar button { position:absolute; right:7px; top:4px; border:0; border-radius:5px; background:transparent; color:#777; font-size:20px; line-height:21px; cursor:pointer; }.drag-bar button:hover { color:#d33; background:#ffe3e3; }.splitter { height:6px; flex:none; display:flex; align-items:center; justify-content:center; background:rgba(0,0,0,.05); cursor:row-resize; }.splitter i { width:28px; height:2px; border-radius:2px; background:rgba(0,0,0,.2); } footer { flex:none; border-top:1px solid rgba(0,0,0,.08); padding:10px; text-align:center; background:rgba(255,255,255,.9); } footer button { border:0; border-radius:8px; min-width:104px; padding:7px 18px; background:#4f7cff; color:#fff; font:13px inherit; cursor:pointer; } footer button:hover { background:#3b67e8; }
</style>
