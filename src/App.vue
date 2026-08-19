<script setup lang="ts">
import { ref } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import ConsoleView from "./components/console/ConsoleView.vue";
import StickerWindow from "./components/sticker/StickerWindow.vue";
import TodoWindow from "./components/todo/TodoWindow.vue";

// 多窗口同构加载：按窗口 label 分发。
// label 以 "sticker-" 开头 → 便签窗口；否则 → 主控台。
function resolveView(label: string): "console" | "sticker" | "todo" {
  return label.startsWith("todo-") ? "todo" : label.startsWith("sticker-") ? "sticker" : "console";
}

// 新窗口首帧直接按 label 路由，避免 Todo 窗口先挂载空白主控台。
const view = ref(resolveView(getCurrentWindow().label));
</script>

<template>
  <ConsoleView v-if="view === 'console'" />
  <StickerWindow v-else-if="view === 'sticker'" />
  <TodoWindow v-else />
</template>
