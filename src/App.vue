<script setup lang="ts">
import { ref, onMounted } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import Console from "./components/demo/Console.vue";
import StickerWindow from "./components/demo/StickerWindow.vue";

// 多窗口同构加载：按窗口 label 分发。
// label 以 "sticker-" 开头 → 便签窗口；否则 → 主控台。
const view = ref<"console" | "sticker">("console");

onMounted(async () => {
  const label = getCurrentWindow().label;
  view.value = label.startsWith("sticker-") ? "sticker" : "console";
});
</script>

<template>
  <Console v-if="view === 'console'" />
  <StickerWindow v-else />
</template>
