<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke, listen } from "../../composables/useTauri";
import { useNotesStore } from "../../stores/notes";
import { usePrefsStore } from "../../stores/prefs";
import { hexToRgba } from "../../utils/markdown";
import type { Sticker, StickerMode } from "../../types";
import type { UnlistenFn } from "@tauri-apps/api/event";
import StickerHeader from "./StickerHeader.vue";
import StickerViewer from "./StickerViewer.vue";
import StickerEditor from "./StickerEditor.vue";
import StickerSettings from "./StickerSettings.vue";

const notes = useNotesStore();
const prefs = usePrefsStore();

const label = getCurrentWindow().label;
const stickerId = Number(label.replace("sticker-", ""));

const sticker = ref<Sticker | null>(null);
const mode = ref<StickerMode>("display");
const showSettings = ref(false);
const alertActive = ref(false);
const unlisteners: UnlistenFn[] = [];
let collapseTimer: number | undefined;

const cardStyle = computed(() => {
  const bg = prefs.effective?.bg_color ?? sticker.value?.bg_color ?? "#FFF4D6";
  // 背景半透明、文字不透明：按模式调整背景 alpha（display 收起最淡，
  // edit 稍实，interact 居中）；文字颜色恒不透明。
  const alpha = mode.value === "display" ? 0.35 : mode.value === "edit" ? 0.95 : 0.9;
  return {
    background: hexToRgba(bg, alpha),
    color: prefs.effective?.text_color ?? "#222222",
  };
});

const bodyFontSize = computed(() => prefs.effective?.body_font_size ?? 13);

async function load() {
  sticker.value = (await invoke<Sticker | null>("get_sticker_cmd", { id: stickerId })) ?? null;
  if (sticker.value) {
    mode.value = (sticker.value.display_mode as StickerMode) || "display";
    await prefs.load(stickerId);
    applyMode(mode.value);
  }
}

/** 应用模式：持久化 + 窗口透明度 + 自动收起计时。 */
async function applyMode(next: StickerMode) {
  mode.value = next;
  if (sticker.value) {
    await invoke("update_sticker_cmd", {
      id: stickerId,
      patch: { display_mode: next },
    });
  }
  // display=低透明收起：背景 alpha 由 cardStyle 控制（文字始终不透明）；
  // interact 5s 无操作自动收起；edit 不自动收起
  resetCollapseTimer();
}

/** interact 模式 5s 无操作自动收起回 display（编辑态不收起）。 */
function resetCollapseTimer() {
  if (collapseTimer) window.clearTimeout(collapseTimer);
  if (mode.value !== "interact") return;
  collapseTimer = window.setTimeout(() => {
    if (mode.value === "interact") applyMode("display");
  }, 5000);
}

function onInteract() {
  resetCollapseTimer();
}

function onDoubleClickWake() {
  if (mode.value === "display") applyMode("interact");
}

function onEnterEdit() {
  if (collapseTimer) window.clearTimeout(collapseTimer);
  applyMode("edit");
}

async function onSaved() {
  await load();
  applyMode("interact");
}

async function onClosed() {
  await notes.remove(stickerId);
  const win = getCurrentWindow();
  await win.close();
}

onMounted(async () => {
  await load();
  // 提醒触发状态信号（无动画，仅提示）
  unlisteners.push(
    await listen<boolean>("sticky://alert-active", (active) => {
      alertActive.value = active;
    }),
  );
  // 内容变更推送 → 重新加载
  unlisteners.push(
    await listen<number>("sticky://push-update", (id) => {
      if (id === stickerId) load();
    }),
  );
});

onBeforeUnmount(() => {
  if (collapseTimer) window.clearTimeout(collapseTimer);
  unlisteners.forEach((u) => u());
});
</script>

<template>
  <div
    class="sticker"
    :style="cardStyle"
    :class="{ display: mode === 'display', alert: alertActive }"
    @dblclick="onDoubleClickWake"
    @click="onInteract"
    @keydown="onInteract"
    @mousemove="onInteract"
  >
    <StickerHeader
      :mode="mode"
      :title="sticker?.title ?? ''"
      @enter-edit="onEnterEdit"
      @toggle-settings="showSettings = !showSettings"
      @close="onClosed"
    />

    <div class="body" :style="{ fontSize: bodyFontSize + 'px' }">
      <StickerViewer
        v-if="mode === 'display' || mode === 'interact'"
        :content="sticker?.content ?? ''"
        :interactive="mode === 'interact'"
        @toggle="(line) => invoke('toggle_todo_cmd', { id: stickerId, line })"
        @pointer="onInteract"
      />
      <StickerEditor
        v-else-if="mode === 'edit'"
        :content="sticker?.content ?? ''"
        :title="sticker?.title ?? ''"
        :sticker-id="stickerId"
        @saved="onSaved"
        @cancelled="() => applyMode('interact')"
      />
    </div>

    <StickerSettings
      v-if="showSettings"
      :sticker-id="stickerId"
      @close="showSettings = false"
    />
  </div>
</template>

<style scoped>
.sticker {
  height: 100vh;
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  box-shadow: 0 6px 24px rgba(0, 0, 0, 0.2);
  transition: box-shadow 0.2s;
}

.sticker.display {
  box-shadow: 0 2px 10px rgba(0, 0, 0, 0.12);
}

.sticker.alert {
  box-shadow: 0 0 0 3px #ff9f43, 0 6px 24px rgba(0, 0, 0, 0.2);
}

.body {
  flex: 1;
  overflow-y: auto;
  padding: 12px 16px;
}
</style>
