<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke, listen } from "../../composables/useTauri";
import { usePrefsStore } from "../../stores/prefs";
import { useSettingsStore } from "../../stores/settings";
import { hexToRgba } from "../../utils/markdown";
import {
  createAutoScrollCursor,
  stepAutoScroll,
  type AutoScrollCursor,
} from "../../utils/auto-scroll";
import type { Sticker, StickerMode, TodoBlock } from "../../types";
import type { UnlistenFn } from "@tauri-apps/api/event";
import StickerViewer from "./StickerViewer.vue";
import StickerEditor from "./StickerEditor.vue";
import StickerSettings from "./StickerSettings.vue";

const editorRef = ref<InstanceType<typeof StickerEditor> | null>(null);

const prefs = usePrefsStore();
const settings = useSettingsStore();

const label = getCurrentWindow().label;
const stickerId = Number(label.replace("sticker-", ""));

const sticker = ref<Sticker | null>(null);
const todoBlocks = ref<TodoBlock[]>([]);
const mode = ref<StickerMode>("display");
const showSettings = ref(false);
/** Todo 窗口在场（用户正在编辑 Todo）：interact 期间不自动收起回展示模式。 */
const todoPresent = ref(false);
const unlisteners: UnlistenFn[] = [];
let collapseTimer: number | undefined;
/** 模式切换提示（应用内 toast，2 秒自动消失，替代系统通知）。 */
const modeToast = ref("");
let toastTimer: number | undefined;

const cardStyle = computed(() => {
  const bg = prefs.effective?.bg_color ?? sticker.value?.bg_color ?? "#FFF4D6";
  // 背景半透明、文字不透明：始终以用户设置的 opacity 为准（展示/交互/编辑一致），
  // 不再对 display 额外减淡；文字颜色恒不透明。
  const alpha = prefs.effective?.opacity ?? 0.9;
  return {
    background: hexToRgba(bg, alpha),
    color: prefs.effective?.text_color ?? "#222222",
  };
});

const bodyFontSize = computed(() => prefs.effective?.body_font_size ?? 13);
const editFontFamily = computed(() => settings.editFontFamily);

// ── 自动滚动（便签设置 auto_scroll）：仅展示模式 ──
// 行为：向下滚到底 → 自动折返向上 → 滚到顶 → 再折返向下，如此往复。
// 实现要点（相对旧版的修复）：
// 1) 每帧以 el.scrollTop 的【真实位置】计算下一步（而非内存里的浮点快照），
//    布局/内容变化、外部滚动都会自动跟随，不存在“到顶后停住”的死区；
// 2) 位移亚像素累积、写入整数 scrollTop，规避 WebView 对小数 scrollTop 的
//    量化短路（低速时连写同一个小数会被当作无变化而停滞）；
// 3) 窗口隐藏恢复后 rAF 首帧的巨长时间差被裁剪，不会瞬跳；
// 4) 内容不足一屏或速度为 0 时原地待命，内容变长后自动恢复。
const bodyRef = ref<HTMLElement | null>(null);
const autoScroll = computed(() => sticker.value?.auto_scroll ?? false);
const autoScrollSpeed = computed(
  () => prefs.effective?.auto_scroll_speed ?? settings.autoScrollSpeed,
);
let scrollRaf: number | undefined;
let lastScrollAt: number | undefined;
let scrollCursor: AutoScrollCursor = createAutoScrollCursor(1);

function tickScroll(timestamp: number) {
  const el = bodyRef.value;
  if (el) {
    const max = el.scrollHeight - el.clientHeight;
    if (max > 0 && autoScrollSpeed.value > 0) {
      if (lastScrollAt === undefined) {
        // 首帧只登记时间基准，不做位移（避免起步跳变）
        lastScrollAt = timestamp;
      } else {
        let dtMs = timestamp - lastScrollAt;
        lastScrollAt = timestamp;
        // rAF 在窗口隐藏/恢复时会挂起，恢复首帧 dt 可能高达数秒：
        // 裁剪到 ≤250ms，恢复显示时继续平滑滚动而非瞬跳。
        dtMs = Math.min(dtMs, 250);
        if (dtMs > 0) {
          const stepPx = (autoScrollSpeed.value * dtMs) / 1000;
          const next = stepAutoScroll(scrollCursor, el.scrollTop, max, stepPx);
          scrollCursor = next.cursor;
          // 只写有意义的整数位移；getter 返回小数（个别 WebView）时
          // 与目标差不足 1px 视为已到位，避免无意义重写。
          if (Math.abs(el.scrollTop - next.scrollTop) >= 1) {
            el.scrollTop = next.scrollTop;
          }
        }
      }
    } else {
      // 内容不足一屏 / 速度为 0：原地待命（保持向下方向，内容变长后自动恢复）
      scrollCursor = createAutoScrollCursor(1);
      lastScrollAt = timestamp;
    }
  } else {
    lastScrollAt = undefined;
  }
  scrollRaf = requestAnimationFrame((nextTimestamp) => tickScroll(nextTimestamp));
}

function startAutoScroll() {
  stopAutoScroll();
  if (autoScroll.value && mode.value === "display") {
    scrollCursor = createAutoScrollCursor(1);
    lastScrollAt = undefined;
    scrollRaf = requestAnimationFrame((timestamp) => tickScroll(timestamp));
  }
}

function stopAutoScroll() {
  if (scrollRaf !== undefined) cancelAnimationFrame(scrollRaf);
  scrollRaf = undefined;
  lastScrollAt = undefined;
}

watch([autoScroll, autoScrollSpeed, mode], startAutoScroll);

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
    todoBlocks.value = await invoke<TodoBlock[]>("list_todo_for_sticker_cmd", { stickerId });
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
  // 3) 模式切换提示：应用内 toast（2 秒），不再弹系统通知
  if (prev !== next && sticker.value) {
    const title = sticker.value.title || `便签 #${stickerId}`;
    modeToast.value = next === "display"
      ? `${title} 进入展示模式`
      : next === "interact"
        ? `${title} 进入交互模式`
        : `${title} 进入编辑模式`;
    if (toastTimer) window.clearTimeout(toastTimer);
    toastTimer = window.setTimeout(() => {
      modeToast.value = "";
    }, 2000);
  }
  resetCollapseTimer();
}

/** interact 模式无操作自动收起回 display；编辑态或设置打开时不收起。
 *  收起秒数可在系统设置修改（auto_collapse_secs，默认 5）。 */
function resetCollapseTimer() {
  if (collapseTimer) window.clearTimeout(collapseTimer);
  if (mode.value !== "interact" || showSettings.value || todoPresent.value) return;
  const secs = settings.autoCollapseSecs;
  collapseTimer = window.setTimeout(() => {
    if (mode.value === "interact" && !showSettings.value) applyMode("display");
  }, secs * 1000);
}

// 设置面板打开/关闭：打开时暂停自动收起，关闭时重新计时
watch(showSettings, (open) => {
  if (open) {
    if (collapseTimer) window.clearTimeout(collapseTimer);
  } else {
    resetCollapseTimer();
  }
});

function onInteract() {
  resetCollapseTimer();
}

async function startDragging(event: MouseEvent) {
  if (event.button !== 0) return;
  event.preventDefault();
  try {
    await getCurrentWindow().startDragging();
  } catch (error) {
    console.error("[sticker] 启动窗口拖动失败：", error);
  }
}

/** 双击：interact→edit 编辑（display 穿透唤醒由全局鼠标钩子驱动）。 */
function onDblClick() {
  if (mode.value === "interact") {
    applyMode("edit");
  }
}

/** 编辑模式形态（及时预览 | Markdown，全局配置 editor_mode）。 */
const editorMode = computed(() => settings.get("editor_mode", "markdown"));

function setEditorMode(v: "markdown" | "live") {
  settings.set("editor_mode", v);
}

/** ✎ 按钮：编辑模式外进入编辑；编辑模式内自动保存并退出（颜色恢复默认）。 */
function onEditBtn() {
  if (mode.value === "edit") {
    // save() 成功会 emit saved → onSaved → applyMode('interact')；失败留在编辑态
    editorRef.value?.save();
  } else {
    applyMode("edit");
  }
}

/** E 键进入编辑（非编辑态）；Ctrl+S 保存（编辑态）。 */
function onKeydown(e: KeyboardEvent) {
  if ((e.ctrlKey || e.metaKey) && (e.key === "s" || e.key === "S")) {
    e.preventDefault();
    if (mode.value === "edit") {
      editorRef.value?.save();
    }
    return;
  }
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
  // 关闭=隐藏窗口（不删除数据）；由 Rust hide_sticker_cmd 隐藏并广播
  // push-update，主控台收到后把按钮切到"显示"，点"显示"再经 wake 恢复。
  if (mode.value === "edit") await editorRef.value?.discard();
  await invoke("hide_sticker_cmd", { id: stickerId });
  releaseData();
}

/** 隐藏时释放内容引用（内存策略）：下次打开由 sticky://wake 或恢复流程重新 load。 */
function releaseData() {
  sticker.value = null;
  todoBlocks.value = [];
}

onMounted(async () => {
  await load();
  unlisteners.push(
    await listen<number>("sticky://push-update", (id) => {
      if (id === stickerId) load();
    }),
  );
  unlisteners.push(
    await listen<string>("todo://updated", () => {
      invoke<TodoBlock[]>("list_todo_for_sticker_cmd", { stickerId }).then((items) => {
        todoBlocks.value = items;
      });
    }),
  );
  unlisteners.push(
    await listen<boolean>("sticky://todo-presence", (present) => {
      todoPresent.value = present;
      resetCollapseTimer();
    }),
  );
  // 全局鼠标钩子中键+左键唤醒（display 穿透状态）→ 前端切换 interact。
  // 事件经 emit_to 定向发送到本窗口，无需校验 id（旧实现校验导致
  // payload 为 unit 时恒不匹配，唤醒后 UI 不切换）。
  unlisteners.push(
    await listen<number>("sticky://wake", async () => {
      if (mode.value === "display") {
        // 隐藏期间数据已释放（releaseData），先重载再切交互。
        if (!sticker.value) await load();
        applyMode("interact");
      }
    }),
  );
});

onBeforeUnmount(() => {
  if (collapseTimer) window.clearTimeout(collapseTimer);
  stopAutoScroll();
  unlisteners.forEach((u) => u());
});
</script>

<template>
  <div
    class="sticker"
    :style="cardStyle"
    :class="{ display: mode === 'display' }"
    tabindex="0"
    @dblclick="onDblClick"
    @click="onInteract"
    @mousemove="onInteract"
    @keydown="onKeydown"
  >
    <!-- 透明顶部蒙版：interact/edit 模式覆盖在内容上方（不占布局空间），
         可拖动窗口（无背景色）；编辑模式额外有 保存 + 编辑模式开关（居左） -->
    <div
      v-if="mode === 'interact' || mode === 'edit'"
      class="overlay"
      :class="{ editing: mode === 'edit' }"
      @mousemove.stop="onInteract"
      @mousedown.self="startDragging"
    >
      <!-- 编辑模式：保存 + 及时预览|Markdown 开关居左 -->
      <div v-if="mode === 'edit'" class="overlay-left">
        <button class="ov-btn save" title="保存（Ctrl+S）" @click.stop="editorRef?.save()">保存</button>
        <div class="editor-switch" title="编辑模式">
          <button class="sw" :class="{ on: editorMode === 'live' }" @click.stop="setEditorMode('live')">及时预览</button>
          <button class="sw" :class="{ on: editorMode === 'markdown' }" @click.stop="setEditorMode('markdown')">Markdown</button>
        </div>
      </div>
      <div class="overlay-right">
        <button
          class="ov-btn"
          :class="{ active: mode === 'edit' }"
          :title="mode === 'edit' ? '保存并退出编辑' : '编辑（E 或双击内容）'"
          @click.stop="onEditBtn"
        ><i class="ri-edit-line"></i></button>
        <button v-if="mode !== 'edit'" class="ov-btn" title="收起回展示模式" @click.stop="applyMode('display')"><i class="ri-arrow-down-s-line"></i></button>
        <button class="ov-btn" title="设置" @click.stop="showSettings = true"><i class="ri-settings-3-line"></i></button>
        <button class="ov-btn close" title="关闭窗口" @click.stop="onClosed"><i class="ri-close-line"></i></button>
      </div>
    </div>

    <div
      ref="bodyRef"
      class="body"
      :class="{ editing: mode === 'edit' }"
      :style="{ fontSize: bodyFontSize + 'px', fontFamily: mode === 'edit' ? editFontFamily : undefined }"
    >
      <StickerViewer
        v-if="mode === 'display' || mode === 'interact'"
        :content="sticker?.content ?? ''"
        :interactive="mode === 'interact'"
        :todo-blocks="todoBlocks"
        @toggle="(line) => invoke('toggle_todo_cmd', { id: stickerId, line })"
        @open-todo="(id) => invoke('open_todo_window_cmd', { id })"
        @toggle-todo="(id, isCompleted) => invoke('update_todo_block_cmd', { id, patch: { is_completed: isCompleted } })"
        @pointer="onInteract"
      />
      <StickerEditor
        v-else-if="mode === 'edit'"
        ref="editorRef"
        :content="sticker?.content ?? ''"
        :sticker-id="stickerId"
        :todo-blocks="todoBlocks"
        @saved="onSaved"
        @cancelled="() => applyMode('interact')"
      />
    </div>

    <StickerSettings v-if="showSettings" :sticker-id="stickerId" @close="showSettings = false" />

    <!-- 模式切换提示（2 秒自动消失） -->
    <transition name="toast">
      <div v-if="modeToast" class="mode-toast">{{ modeToast }}</div>
    </transition>
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

.body {
  flex: 1;
  overflow-y: auto;
  padding: 14px 16px;
}

/* 编辑模式：顶部留出按钮条空间，避免与内容重叠 */
.body.editing {
  padding-top: 46px;
}

/* ── 透明顶部蒙版（interact/edit 模式） ── */
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

/* 编辑模式：保存/取消居左，四按钮居右 */
.overlay.editing {
  justify-content: space-between;
  padding-right: 6px;
}

.overlay-left,
.overlay-right {
  display: flex;
  align-items: center;
  gap: 2px;
}

.overlay-left {
  padding-left: 6px;
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
  font-size: 15px;
  color: #555;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  box-shadow: 0 1px 4px rgba(0, 0, 0, 0.15);
}

/* 文字按钮（保存）：与 ov-btn 同风格，宽度自适应 */
.ov-btn.save {
  width: auto;
  padding: 0 10px;
  font-size: 12px;
  background: #4f7cff;
  color: #fff;
}

.ov-btn.save:hover {
  background: #3b67e8;
  color: #fff;
}

/* 编辑模式：✎ 蓝色高亮 */
.ov-btn.active {
  background: #4f7cff;
  color: #fff;
}

.ov-btn.active:hover {
  background: #3b67e8;
  color: #fff;
}

.ov-btn:hover {
  background: #fff;
  color: #4f7cff;
}

.ov-btn.close:hover {
  background: #ffe3e3;
  color: #d33;
}

/* 编辑模式开关：及时预览 | Markdown */
.editor-switch {
  display: flex;
  background: rgba(255, 255, 255, 0.75);
  border-radius: 6px;
  padding: 2px;
  box-shadow: 0 1px 4px rgba(0, 0, 0, 0.15);
}

.editor-switch .sw {
  border: none;
  background: transparent;
  font-size: 11px;
  padding: 2px 8px;
  border-radius: 4px;
  color: #666;
  cursor: pointer;
  white-space: nowrap;
}

.editor-switch .sw:hover {
  color: #4f7cff;
}

.editor-switch .sw.on {
  background: #4f7cff;
  color: #fff;
}

.editor-switch .sw.on:hover {
  background: #3b67e8;
  color: #fff;
}

/* ── 模式切换提示 toast（2 秒） ── */
.mode-toast {
  position: absolute;
  left: 50%;
  bottom: 18px;
  transform: translateX(-50%);
  background: rgba(40, 40, 40, 0.82);
  color: #fff;
  font-size: 12.5px;
  padding: 7px 16px;
  border-radius: 999px;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
  white-space: nowrap;
  pointer-events: none;
  z-index: 40;
}

.toast-enter-active,
.toast-leave-active {
  transition: opacity 0.25s ease, transform 0.25s ease;
}

.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translateX(-50%) translateY(8px);
}
</style>
