<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { TodoBlock, TodoPatch } from "../../types";
import { duePreset, formatTodoDate, reminderPreset, type TodoPresetConfig } from "../../utils/todo-dates";
import TodoDatePicker from "./TodoDatePicker.vue";
import RepeatPicker from "./RepeatPicker.vue";

const props = defineProps<{ item: TodoBlock | null; presets: TodoPresetConfig }>();
const emit = defineEmits<{ patch: [patch: TodoPatch]; createChild: [] }>();
const title = ref(""); const description = ref("");
const openPicker = ref<"reminder" | "due" | "repeat" | null>(null);
const isChild = computed(() => Boolean(props.item?.parent_id));
const repeatLabel = computed(() => {
  if (!props.item?.repeat_rule) return "未设置";
  try { const rule = JSON.parse(props.item.repeat_rule) as { unit?: string; interval?: number }; return `每 ${rule.interval ?? 1} ${{ day: "天", week: "周", month: "月", year: "年" }[rule.unit ?? ""] ?? ""}`.trim(); } catch { return "未设置"; }
});
watch(() => props.item, (item) => { title.value = item?.title ?? ""; description.value = item?.description ?? ""; openPicker.value = null; }, { immediate: true });
function updateText() { emit("patch", { title: title.value, description: description.value }); }
function setReminder(value: string) { emit("patch", { reminder_at: value }); openPicker.value = null; }
function setDue(value: string) { emit("patch", { due_at: value }); openPicker.value = null; }
function setRepeat(value: string) { emit("patch", { repeat_rule: value }); openPicker.value = null; }
function togglePicker(kind: "reminder" | "due" | "repeat") { openPicker.value = openPicker.value === kind ? null : kind; }
</script>

<template>
  <section class="todo-lower" @click.self="openPicker = null">
    <h2 class="todo-title">{{ isChild ? "子任务详情" : "任务详情" }}</h2>
    <div v-if="item" class="fields">
      <label class="field"><span class="field-label">任务名称</span><input v-model="title" type="text" placeholder="输入任务名称" @input="updateText" /></label>
      <button v-if="!isChild" class="add-child-btn" @click="emit('createChild')">+ 添加子任务</button>
      <label class="field"><span class="field-label">任务描述</span><textarea v-model="description" rows="3" placeholder="输入任务描述" @input="updateText"></textarea></label>
      <template v-if="!isChild">
        <div class="field picker-field"><span class="field-label">提醒时间</span><div class="chips"><button @click="setReminder(reminderPreset('hour', presets))">1小时后</button><button @click="setReminder(reminderPreset('tomorrow', presets))">明天</button><button @click="setReminder(reminderPreset('next-week', presets))">下周</button><button :class="{ active: openPicker === 'reminder', empty: !item.reminder_at }" @click="togglePicker('reminder')">{{ formatTodoDate(item.reminder_at) }} ▾</button></div><TodoDatePicker v-if="openPicker === 'reminder'" :value="item.reminder_at" :with-time="true" @save="setReminder" @cancel="openPicker = null" /></div>
        <div class="field picker-field"><span class="field-label">截至时间</span><div class="chips"><button @click="setDue(duePreset('today', presets))">今天</button><button @click="setDue(duePreset('tomorrow', presets))">明天</button><button @click="setDue(duePreset('next-week', presets))">下周</button><button :class="{ active: openPicker === 'due', empty: !item.due_at }" @click="togglePicker('due')">{{ formatTodoDate(item.due_at, false) }} ▾</button></div><TodoDatePicker v-if="openPicker === 'due'" :value="item.due_at" :with-time="false" @save="setDue" @cancel="openPicker = null" /></div>
        <div class="field picker-field"><span class="field-label">设置任务重复</span><div class="chips"><button @click="setRepeat(JSON.stringify({ unit: 'day', interval: 1 }))">每天</button><button @click="setRepeat(JSON.stringify({ unit: 'week', interval: 1 }))">每周</button><button @click="setRepeat(JSON.stringify({ unit: 'month', interval: 1 }))">每月</button><button @click="setRepeat(JSON.stringify({ unit: 'year', interval: 1 }))">每年</button><button :class="{ active: openPicker === 'repeat', empty: !item.repeat_rule }" @click="togglePicker('repeat')">{{ repeatLabel }} ▾</button></div><RepeatPicker v-if="openPicker === 'repeat'" :value="item.repeat_rule" @save="setRepeat" @cancel="openPicker = null" /></div>
      </template>
      <p v-else class="sub-hint">子任务不继承高级设置，也不能添加自己的子任务。</p>
    </div>
    <p v-else class="empty">选择或新建一个任务以开始编辑。</p>
  </section>
</template>

<style scoped>
.todo-lower { flex:1; overflow:auto; padding:14px 16px; box-sizing:border-box; background:rgba(255,255,255,.82); }.todo-title { margin:0 0 10px; padding-bottom:8px; border-bottom:1px solid rgba(0,0,0,.08); color:#333; font-size:14px; }.field { display:block; position:relative; padding:8px 0; color:#888; font-size:12.5px; }.field-label { display:block; margin-bottom:6px; }.field input,.field textarea { box-sizing:border-box; width:100%; border:1px solid rgba(0,0,0,.15); border-radius:6px; background:#fff; color:#333; padding:7px 8px; font:13px inherit; resize:vertical; }.add-child-btn { margin:2px 0; padding:5px 8px; border:0; background:none; color:#4f7cff; font:12px inherit; cursor:pointer; }.chips { display:flex; flex-wrap:wrap; gap:6px; }.chips button { border:1px solid rgba(0,0,0,.12); border-radius:6px; background:rgba(255,255,255,.6); color:#444; padding:5px 9px; font:12px inherit; cursor:pointer; }.chips button:hover { background:#fff; border-color:rgba(0,0,0,.2); }.chips button.active { color:#fff; border-color:#4f7cff; background:#4f7cff; }.chips button.empty { border-style:dashed; color:#999; }.chips button.empty:hover { color:#4f7cff; }.sub-hint,.empty { margin:10px 0; color:#999; font-size:12px; }.empty { text-align:center; padding-top:30px; }
</style>
