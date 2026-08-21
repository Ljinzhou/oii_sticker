<script setup lang="ts">
import { ref, onMounted } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "./composables/useTauri";
import { useNotesStore } from "./stores/notes";
import ConsoleView from "./components/console/ConsoleView.vue";
import OnboardingDialog from "./components/console/OnboardingDialog.vue";
import StickerWindow from "./components/sticker/StickerWindow.vue";
import TodoWindow from "./components/todo/TodoWindow.vue";

// 多窗口同构加载：按窗口 label 分发。
// label 以 "sticker-" 开头 → 便签窗口；否则 → 主控台。
function resolveView(label: string): "console" | "sticker" | "todo" {
  return label.startsWith("todo-") ? "todo" : label.startsWith("sticker-") ? "sticker" : "console";
}

// 新窗口首帧直接按 label 路由，避免 Todo 窗口先挂载空白主控台。
const view = ref(resolveView(getCurrentWindow().label));

// 首次启动引导：无任何工作控件时弹窗创建第一个。
const notes = useNotesStore();
const showOnboarding = ref(false);

onMounted(async () => {
  if (view.value !== "console") return;
  try {
    const list = await invoke<unknown[]>("workspace_list_cmd");
    if (list.length === 0) showOnboarding.value = true;
  } catch (e) {
    console.warn("[boot] 工作控件列表查询失败：", e);
  }
});

async function onOnboardingDone() {
  showOnboarding.value = false;
  await notes.refresh();
}
</script>

<template>
  <ConsoleView v-if="view === 'console'" />
  <StickerWindow v-else-if="view === 'sticker'" />
  <TodoWindow v-else />
  <OnboardingDialog v-if="showOnboarding" @done="onOnboardingDone" />
</template>
