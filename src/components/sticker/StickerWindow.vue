<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke, listen } from "../../composables/useTauri";
import { usePrefsStore } from "../../stores/prefs";
import { hexToRgba } from "../../utils/markdown";
import type { Sticker, StickerMode } from "../../types";
import type { UnlistenFn } from "@tauri-apps/api/event";
import StickerViewer from "./StickerViewer.vue";
import StickerEditor from "./StickerEditor.vue";
import StickerSettings from "./StickerSettings.vue";

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
  // 背景半透明、文字不透明：alpha 来自用户设置的 opacity（立即生效），
  // display 收起模式在此基础上再减淡；文字颜色恒不透明。
  const base = prefs.effective?.opacity ?? 0.9;
  const alpha = mode.value === "display" ? base * 0.4 : base;
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
    resetCollapseTimer();
  }
}

/** 应用模式：持久化 + 自动收起计时（背景透明度由 cardStyle 控制）。 */
async function applyMode(next: StickerMode) {
  mode.value = next;
  if (sticker.value) {
    await invoke("update_sticker_cmd", {
      id: stickerId,
      patch: { display_mode: next },
    });
  }
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

/** 双击：display→interact 唤醒；interact→edit 编辑。 */
function onDblClick() {
  if (mode.value === "display") {
    applyMode("interact");
  } else if (mode.value === "interact") {
    applyMode("edit");
  }
}

/** E 键进入编辑（非编辑态）。 */
function onKeydown(e: KeyboardEvent) {
  if (mode.value !== "edit" && (e.key === "e" || e.key === "E")) {
    applyMode("edit");
  }
  onInteract();
}

async function onSaved() {
  await load();
  applyMode("interact");
}

async function onClosed() {
  // 只关闭窗口，不删除数据（主控台保留该便签）
  await getCurrentWindow().close();
}

onMounted(async () => {
  await load();
  unlisteners.push(
    await listen<boolean>("sticky://alert-active", (active) => {
      alertActive.value = active;
    }),
  );
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
    tabindex="0"
    @dblclick="onDblClick"
    @click="onInteract"
    @mousemove="onInteract"
    @keydown="onKeydown"
  >
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
        :sticker-id="stickerId"
        @saved="onSaved"
        @cancelled="() => applyMode('interact')"
        @toggle-settings="showSettings = !showSettings"
        @closed="onClosed"
      />
    </div>

    <StickerSettings v-if="showSettings" :sticker-id="stickerId" @close="showSettings = false" />
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
  outline: none;
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
  padding: 14px 16px;
}
</style>
