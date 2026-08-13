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
    // 先应用窗口状态（display 穿透+锁尺寸），不依赖后续加载成功
    // 注意：Tauri 2 invoke 参数为 camelCase（Rust is_display → JS isDisplay）
    try {
      await invoke("apply_window_state_cmd", { id: stickerId, isDisplay: mode.value === "display" });
    } catch (e) {
      console.error("[ui] 应用窗口状态失败：", e);
    }
    await prefs.load(stickerId);
    resetCollapseTimer();
  }
}

/** 应用模式：窗口状态优先（display 穿透+锁尺寸），再持久化，再通知。 */
async function applyMode(next: StickerMode) {
  const prev = mode.value;
  mode.value = next;
  // 1) 窗口状态（独立执行，失败不阻断后续）；参数为 camelCase
  try {
    await invoke("apply_window_state_cmd", { id: stickerId, isDisplay: next === "display" });
  } catch (e) {
    console.error("[ui] 应用窗口状态失败：", e);
  }
  // 2) 持久化模式
  if (sticker.value) {
    try {
      await invoke("update_sticker_cmd", {
        id: stickerId,
        patch: { display_mode: next },
      });
    } catch (e) {
      console.error("[ui] 持久化模式失败：", e);
    }
  }
  // 3) 模式切换系统通知
  if (prev !== next && sticker.value) {
    const title = sticker.value.title || `便签 #${stickerId}`;
    const body = next === "display"
      ? `${title} 进入展示模式`
      : next === "interact"
        ? `${title} 进入交互模式`
        : `${title} 进入编辑模式`;
    invoke("debug_notify_cmd", { title, body });
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

/** 双击：interact→edit 编辑（display 穿透唤醒由全局鼠标钩子驱动）。 */
function onDblClick() {
  if (mode.value === "interact") {
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
  // 全局鼠标钩子中键+左键唤醒（display 穿透状态）→ 前端切换 interact。
  // 事件经 emit_to 定向发送到本窗口，无需校验 id（旧实现校验导致
  // payload 为 unit 时恒不匹配，唤醒后 UI 不切换）。
  unlisteners.push(
    await listen<number>("sticky://wake", () => {
      if (mode.value === "display") {
        applyMode("interact");
      }
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
    <!-- 透明顶部蒙版：interact 模式覆盖在内容上方（不占布局空间），
         显示四功能按钮 + 可拖动窗口（无背景色） -->
    <div
      v-if="mode === 'interact'"
      class="overlay"
      data-tauri-drag-region
      @mousemove.stop="onInteract"
    >
      <button class="ov-btn" title="收起回展示模式" @click.stop="applyMode('display')">▽</button>
      <button class="ov-btn" title="编辑（E 或双击内容）" @click.stop="applyMode('edit')">✎</button>
      <button class="ov-btn" title="设置" @click.stop="showSettings = true">⚙</button>
      <button class="ov-btn close" title="关闭窗口" @click.stop="onClosed">✕</button>
    </div>

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

/* ── 透明顶部蒙版（interact 模式） ── */
.overlay {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: 30px;
  display: flex;
  justify-content: flex-end;
  align-items: center;
  gap: 2px;
  padding-right: 6px;
  background: transparent;
  cursor: grab;
  z-index: 15;
  opacity: 1;
}

.overlay:hover {
  opacity: 1;
}

.ov-btn {
  border: none;
  background: rgba(255, 255, 255, 0.75);
  width: 24px;
  height: 24px;
  border-radius: 6px;
  font-size: 13px;
  color: #555;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  box-shadow: 0 1px 4px rgba(0, 0, 0, 0.15);
}

.ov-btn:hover {
  background: #fff;
  color: #4f7cff;
}

.ov-btn.close:hover {
  background: #ffe3e3;
  color: #d33;
}
</style>
