<script setup lang="ts">
import { ref, onMounted } from "vue";
import { invoke } from "../../composables/useTauri";
import { usePrefsStore } from "../../stores/prefs";

const props = defineProps<{ stickerId: number }>();
const emit = defineEmits<{ close: [] }>();
const prefs = usePrefsStore();

// 偏好（及时生效，无保存按钮）
const opacity = ref(85);
const bgColor = ref("#FFF4D6");
const textColor = ref("#222222");
const bodyFontSize = ref(13);
const alwaysOnTop = ref(false);
const autoScroll = ref(false);
const autoScrollSpeed = ref(30);

let debounceTimer: number | undefined;

function normalizeAutoScrollSpeed(value: number): number {
  if (!Number.isFinite(value)) return 30;
  const clamped = Math.min(120, Math.max(5, value));
  return 5 + Math.round((clamped - 5) / 5) * 5;
}

async function load() {
  await prefs.load(props.stickerId);
  const e = prefs.effective;
  if (e) {
    opacity.value = Math.round(e.opacity * 100);
    bgColor.value = e.bg_color;
    textColor.value = e.text_color;
    bodyFontSize.value = e.body_font_size;
    autoScrollSpeed.value = normalizeAutoScrollSpeed(e.auto_scroll_speed);
  }
  alwaysOnTop.value = (await invoke<{ always_on_top: boolean; auto_scroll: boolean }>("get_sticker_cmd", { id: props.stickerId }))?.always_on_top ?? false;
  autoScroll.value = (await invoke<{ always_on_top: boolean; auto_scroll: boolean }>("get_sticker_cmd", { id: props.stickerId }))?.auto_scroll ?? false;
}

/** 拖动/输入过程中：本地及时应用（视觉立即生效，无网络往返）。 */
function applyPrefsSoon() {
  autoScrollSpeed.value = normalizeAutoScrollSpeed(autoScrollSpeed.value);
  prefs.applyLocal({
    opacity: Number(opacity.value) / 100,
    bg_color: bgColor.value,
    text_color: textColor.value,
    body_font_size: bodyFontSize.value,
    auto_scroll_speed: autoScrollSpeed.value,
  });
}

/** 松手/确认后：持久化到后端。 */
function commitPrefs() {
  autoScrollSpeed.value = normalizeAutoScrollSpeed(autoScrollSpeed.value);
  if (debounceTimer) window.clearTimeout(debounceTimer);
  debounceTimer = window.setTimeout(() => {
    prefs.save(props.stickerId, {
      opacity: Number(opacity.value) / 100,
      bg_color: bgColor.value,
      text_color: textColor.value,
      body_font_size: bodyFontSize.value,
      auto_scroll_speed: autoScrollSpeed.value,
    });
  }, 250);
}

function saveAlwaysOnTop(v: boolean) {
  invoke("update_sticker_cmd", { id: props.stickerId, patch: { always_on_top: v } });
}

function saveAutoScroll(v: boolean) {
  invoke("update_sticker_cmd", { id: props.stickerId, patch: { auto_scroll: v } });
}

async function resetPrefs() {
  await prefs.reset(props.stickerId);
  await load();
}

onMounted(load);
</script>

<template>
  <div class="modal-mask" @click.self="emit('close')">
    <div class="panel">
      <header>
        <h2>便签设置</h2>
        <button class="close" @click="emit('close')"><i class="ri-close-line"></i></button>
      </header>

      <section class="group">
        <h3>外观（修改及时生效）</h3>
        <label class="row">
          <span>背景透明度</span>
          <input v-model.number="opacity" type="range" min="15" max="100" @input="applyPrefsSoon" @change="commitPrefs" />
          <span class="val">{{ opacity }}%</span>
        </label>
        <label class="row">
          <span>背景颜色</span>
          <input v-model="bgColor" type="color" @input="applyPrefsSoon" @change="commitPrefs" />
        </label>
        <label class="row">
          <span>文字颜色</span>
          <input v-model="textColor" type="color" @input="applyPrefsSoon" @change="commitPrefs" />
        </label>
        <label class="row">
          <span>正文字号</span>
          <input v-model.number="bodyFontSize" type="number" min="9" max="28" @change="commitPrefs" />
        </label>
        <label class="row">
          <span>窗口置顶</span>
          <input v-model="alwaysOnTop" type="checkbox" @change="saveAlwaysOnTop(alwaysOnTop)" />
        </label>
        <label class="row">
          <span>自动滚动</span>
          <input v-model="autoScroll" type="checkbox" @change="saveAutoScroll(autoScroll)" />
        </label>
        <label class="row">
          <span>滚动速度</span>
          <input
            v-model.number="autoScrollSpeed"
            data-testid="auto-scroll-speed"
            type="range"
            min="5"
            max="120"
            step="5"
            :disabled="!autoScroll"
            @input="applyPrefsSoon"
            @change="commitPrefs"
          />
          <span class="val">{{ autoScrollSpeed }} px/s</span>
        </label>
        <button class="link" @click="resetPrefs"><i class="ri-restart-line"></i> 恢复默认偏好</button>
      </section>

      <footer>
        <button class="btn" @click="emit('close')">取消</button>
      </footer>
    </div>
  </div>
</template>

<style scoped>
.modal-mask {
  position: absolute;
  inset: 0;
  background: rgba(0, 0, 0, 0.25);
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 0;
  z-index: 20;
}

.panel {
  width: 320px;
  max-height: 82%;
  overflow-y: auto;
  background: #fff;
  border-radius: 12px;
  padding: 16px;
  box-shadow: 0 12px 40px rgba(0, 0, 0, 0.3);
}

header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 8px;
}

h2 {
  margin: 0;
  font-size: 15px;
  color: #333;
}

.close {
  border: none;
  background: none;
  font-size: 16px;
  color: #999;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
}

.link {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.group {
  margin-top: 12px;
}

h3 {
  margin: 0 0 6px;
  font-size: 12px;
  color: #888;
}

.row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 5px 0;
  font-size: 13px;
  color: #444;
}

.row input[type="range"] {
  flex: 1;
  accent-color: #4f7cff;
}

.row input[type="number"] {
  width: 56px;
  border: 1px solid rgba(0, 0, 0, 0.15);
  border-radius: 6px;
  padding: 3px 6px;
}

.val {
  min-width: 34px;
  text-align: right;
  font-size: 12px;
  color: #888;
}

.link {
  border: none;
  background: none;
  color: #4f7cff;
  font-size: 12px;
  cursor: pointer;
  padding: 6px 0;
}

footer {
  margin-top: 14px;
  display: flex;
  gap: 8px;
  justify-content: flex-end;
}

.btn {
  border: 1px solid rgba(0, 0, 0, 0.12);
  border-radius: 8px;
  padding: 7px 14px;
  font-size: 13px;
  background: #fff;
  color: #333;
  cursor: pointer;
}
</style>
