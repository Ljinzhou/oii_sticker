<script setup lang="ts">
import { ref, onMounted } from "vue";
import { invoke } from "../../composables/useTauri";
import { usePrefsStore } from "../../stores/prefs";
import type { StickerAttrs } from "../../types";

const props = defineProps<{ stickerId: number }>();
const emit = defineEmits<{ close: [] }>();
const prefs = usePrefsStore();

// 偏好
const opacity = ref(85);
const bgColor = ref("#FFF4D6");
const textColor = ref("#222222");
const titleFontSize = ref(14);
const bodyFontSize = ref(13);
const alwaysOnTop = ref(false);

// 提醒
const remindAt = ref("");
const remindRule = ref("");
const isRecurring = ref(false);

async function load() {
  await prefs.load(props.stickerId);
  const e = prefs.effective;
  if (e) {
    opacity.value = Math.round(e.opacity * 100);
    bgColor.value = e.bg_color;
    textColor.value = e.text_color;
    titleFontSize.value = e.title_font_size;
    bodyFontSize.value = e.body_font_size;
  }
  // 读取当前提醒
  try {
    const attrs = await invoke<StickerAttrs | null>("get_reminder_cmd", { id: props.stickerId });
    if (attrs) {
      remindAt.value = attrs.remind_at ?? "";
      remindRule.value = attrs.remind_rule ?? "";
      isRecurring.value = attrs.is_recurring;
    }
  } catch {
    /* 忽略 */
  }
}

async function savePrefs() {
  await prefs.save(props.stickerId, {
    opacity: Number(opacity.value) / 100,
    bg_color: bgColor.value,
    text_color: textColor.value,
    title_font_size: titleFontSize.value,
    body_font_size: bodyFontSize.value,
  });
}

async function saveReminder() {
  if (remindAt.value) {
    await invoke("set_reminder_cmd", {
      attrs: {
        sticker_id: props.stickerId,
        due_date: null,
        remind_at: remindAt.value,
        remind_rule: remindRule.value || null,
        is_recurring: isRecurring.value,
      },
    });
  } else {
    await invoke("clear_reminder_cmd", { id: props.stickerId });
  }
}

async function resetPrefs() {
  await prefs.reset(props.stickerId);
  await load();
}

async function saveAll() {
  await savePrefs();
  await saveReminder();
  await invoke("update_sticker_cmd", {
    id: props.stickerId,
    patch: { always_on_top: alwaysOnTop.value },
  });
  emit("close");
}

onMounted(load);
</script>

<template>
  <div class="modal-mask" @click.self="emit('close')">
    <div class="panel">
      <header>
        <h2>便签设置</h2>
        <button class="close" @click="emit('close')">✕</button>
      </header>

      <section class="group">
        <h3>外观</h3>
        <label class="row">
          <span>背景透明度</span>
          <input v-model.number="opacity" type="range" min="15" max="100" />
          <span class="val">{{ opacity }}%</span>
        </label>
        <label class="row">
          <span>背景颜色</span>
          <input v-model="bgColor" type="color" />
        </label>
        <label class="row">
          <span>文字颜色</span>
          <input v-model="textColor" type="color" />
        </label>
        <label class="row">
          <span>标题字号</span>
          <input v-model.number="titleFontSize" type="number" min="10" max="32" />
        </label>
        <label class="row">
          <span>正文字号</span>
          <input v-model.number="bodyFontSize" type="number" min="9" max="28" />
        </label>
        <label class="row">
          <span>窗口置顶</span>
          <input v-model="alwaysOnTop" type="checkbox" />
        </label>
        <button class="link" @click="resetPrefs">恢复默认偏好</button>
      </section>

      <section class="group">
        <h3>提醒</h3>
        <label class="row">
          <span>提醒时间</span>
          <input v-model="remindAt" type="datetime-local" step="60" />
        </label>
        <label class="row">
          <span>重复规则</span>
          <select v-model="remindRule">
            <option value="">不重复</option>
            <option value="daily">每天</option>
            <option value="weekly">每周（周一）</option>
            <option value="interval:2">每 2 天</option>
            <option value="monthly:1">每月 1 号</option>
            <option value="yearly:12-25">每年 12-25</option>
          </select>
        </label>
        <label class="row">
          <span>循环提醒</span>
          <input v-model="isRecurring" type="checkbox" />
        </label>
      </section>

      <footer>
        <button class="btn primary" @click="saveAll">保存</button>
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
  border-radius: 14px;
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
  font-size: 14px;
  color: #999;
  cursor: pointer;
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

.row input[type="datetime-local"],
.row select {
  border: 1px solid rgba(0, 0, 0, 0.15);
  border-radius: 6px;
  padding: 4px 6px;
  font-size: 12px;
  width: 180px;
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
  padding: 4px 0;
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

.btn.primary {
  background: #4f7cff;
  border-color: #4f7cff;
  color: #fff;
}
</style>
