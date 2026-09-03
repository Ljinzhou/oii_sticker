<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { TodoBlock, TodoPatch } from "../../types";
import { formatTodoDate, formatTodoRepeat } from "../../utils/todo-dates";
import { parsePresetRule, presetToRepeatRule, type PresetItem } from "../../utils/presets";
import { useSettingsStore } from "../../stores/settings";
import TodoDatePicker from "./TodoDatePicker.vue";
import RepeatPicker from "./RepeatPicker.vue";
import PickerFloat from "./PickerFloat.vue";

const props = defineProps<{
  item: TodoBlock | null;
  /** 当前块 id，用于区分「父任务」与「子任务」。 */
  blockId: string;
}>();
const emit = defineEmits<{ patch: [patch: TodoPatch]; createChild: [] }>();
const title = ref(""); const description = ref("");
const openPicker = ref<"reminder" | "due" | "repeat" | null>(null);
const pickerAnchor = ref<DOMRect | null>(null);
let closedByTrigger: EventTarget | null = null;
type ReminderSource = "empty" | "hour" | "tomorrow" | "next-week" | "custom";
type DueSource = "empty" | "today" | "tomorrow" | "next-week" | "custom";
type RepeatSource = "empty" | "day" | "week" | "month" | "year" | "custom";
const reminderSource = ref<ReminderSource>("empty");
const dueSource = ref<DueSource>("empty");
const repeatSource = ref<RepeatSource>("empty");
// 三层结构下「有 parent_id」不再等于「子任务」：
//   parent_id === blockId  → 父任务（可设提醒/截止/重复）
//   parent_id 为其它父任务 → 子任务（仅名称 + 备注）
// 旧写法 Boolean(item.parent_id) 会把父任务误判成子任务，导致父任务丢失提醒功能。
const isSubTask = computed(() => {
  const item = props.item;
  if (!item?.parent_id) return false;
  return item.parent_id !== props.blockId;
});
const reminderTitle = computed(() => props.item?.reminder_at ? `提醒时间 - ${formatTodoDate(props.item.reminder_at)}` : "提醒时间");
const dueTitle = computed(() => props.item?.due_at ? `截至时间 - ${formatTodoDate(props.item.due_at)}` : "截至时间");
const repeatTitle = computed(() => props.item?.repeat_rule ? `设置任务重复时间 - ${formatTodoRepeat(props.item.repeat_rule)}` : "设置任务重复时间");

// 预设列表（系统设置 → Todo 设置 管理），动态渲染三行 chips
const settings = useSettingsStore();
const reminderPresets = computed(() => settings.todoPresets.reminders);
const duePresets = computed(() => settings.todoPresets.due);
const repeatPresets = computed(() => settings.todoPresets.repeats);

/** 点击提醒预设：解析为具体 ISO 写入（循环规则不可用于提醒，忽略）。 */
function applyReminderPreset(item: PresetItem) {
  const iso = parsePresetRule(item.rule);
  if (!iso) return;
  setReminder(iso, `preset-${item.id}`);
}
/** 点击截止预设：同上。 */
function applyDuePreset(item: PresetItem) {
  const iso = parsePresetRule(item.rule);
  if (!iso) return;
  setDue(iso, `preset-${item.id}`);
}
/** 点击重复预设：转为后端 repeat_rule JSON（兼容现有 RepeatPicker 格式）。 */
function applyRepeatPreset(item: PresetItem) {
  const rule = presetToRepeatRule(item.rule);
  if (!rule) return;
  setRepeat(rule, `preset-${item.id}`);
}
watch(() => props.item?.id, () => {
  const item = props.item;
  title.value = item?.title ?? "";
  description.value = item?.description ?? "";
  reminderSource.value = item?.reminder_at ? "custom" : "empty";
  dueSource.value = item?.due_at ? "custom" : "empty";
  repeatSource.value = item?.repeat_rule ? "custom" : "empty";
  openPicker.value = null;
}, { immediate: true });
function updateText() { emit("patch", { title: title.value, description: description.value }); }
function cancelField(kind: "reminder" | "due" | "repeat") {
  if (kind === "reminder") { reminderSource.value = "empty"; emit("patch", { reminder_at: "" }); }
  if (kind === "due") { dueSource.value = "empty"; emit("patch", { due_at: "" }); }
  if (kind === "repeat") { repeatSource.value = "empty"; emit("patch", { repeat_rule: "" }); }
  openPicker.value = null;
}
function fieldHasValue(kind: "reminder" | "due" | "repeat"): boolean {
  return kind === "reminder" ? Boolean(props.item?.reminder_at) : kind === "due" ? Boolean(props.item?.due_at) : Boolean(props.item?.repeat_rule);
}
function fieldSource(kind: "reminder" | "due" | "repeat"): string {
  return kind === "reminder" ? reminderSource.value : kind === "due" ? dueSource.value : repeatSource.value;
}
/** 预设按钮：再次点击当前生效的按钮 → 取消设置（清空对应字段）。 */
function setReminder(value: string, source: ReminderSource = "custom") {
  if (source !== "custom" && reminderSource.value === source && fieldHasValue("reminder")) { cancelField("reminder"); return; }
  reminderSource.value = source; emit("patch", { reminder_at: value }); openPicker.value = null;
}
function setDue(value: string, source: DueSource = "custom") {
  if (source !== "custom" && dueSource.value === source && fieldHasValue("due")) { cancelField("due"); return; }
  dueSource.value = source; emit("patch", { due_at: value }); openPicker.value = null;
}
function setRepeat(value: string, source: RepeatSource = "custom") {
  if (source !== "custom" && repeatSource.value === source && fieldHasValue("repeat")) { cancelField("repeat"); return; }
  repeatSource.value = source; emit("patch", { repeat_rule: value }); openPicker.value = null;
}
function onClosePicker(source?: Event) { closedByTrigger = source?.target instanceof Node ? source.target : null; openPicker.value = null; }
function togglePicker(kind: "reminder" | "due" | "repeat", event?: MouseEvent) {
  if (closedByTrigger && closedByTrigger === event?.currentTarget) { closedByTrigger = null; return; }
  closedByTrigger = null;
  if (openPicker.value === kind) { openPicker.value = null; return; }
  if (!openPicker.value && fieldHasValue(kind) && fieldSource(kind) === "custom") { cancelField(kind); return; }
  pickerAnchor.value = event?.currentTarget instanceof HTMLElement ? event.currentTarget.getBoundingClientRect() : null;
  openPicker.value = kind;
}
</script>

<template>
  <section class="todo-lower" @click.self="openPicker = null" @scroll="openPicker = null">
    <h2 class="todo-title">{{ isSubTask ? "子任务详情" : "任务详情" }}</h2>
    <div v-if="item" class="fields">
      <label class="field"><span class="field-label">任务名称</span><input v-model="title" type="text" placeholder="输入任务名称" @input="updateText" /></label>
      <button v-if="!isSubTask" class="add-child-btn" @click="emit('createChild')"><i class="ri-add-line"></i> 添加子任务</button>
      <label class="field"><span class="field-label">任务描述</span><textarea v-model="description" rows="3" placeholder="输入任务描述" @input="updateText"></textarea></label>
      <template v-if="!isSubTask">
        <div class="field picker-field"><span class="field-label">{{ reminderTitle }}</span><div class="chips"><button v-for="item in reminderPresets" :key="item.id" :class="{ active: reminderSource === 'preset-' + item.id }" @click="applyReminderPreset(item)">{{ item.name }}</button><button :class="{ active: reminderSource === 'custom', empty: reminderSource === 'empty' }" @click="togglePicker('reminder', $event)">自定义</button></div><PickerFloat v-if="openPicker === 'reminder' && pickerAnchor" :anchor="pickerAnchor" @close="onClosePicker"><TodoDatePicker :value="item.reminder_at" :with-time="true" @save="setReminder" @cancel="openPicker = null" /></PickerFloat></div>
        <div class="field picker-field"><span class="field-label">{{ dueTitle }}</span><div class="chips"><button v-for="item in duePresets" :key="item.id" :class="{ active: dueSource === 'preset-' + item.id }" @click="applyDuePreset(item)">{{ item.name }}</button><button :class="{ active: dueSource === 'custom', empty: dueSource === 'empty' }" @click="togglePicker('due', $event)">自定义</button></div><PickerFloat v-if="openPicker === 'due' && pickerAnchor" :anchor="pickerAnchor" @close="onClosePicker"><TodoDatePicker :value="item.due_at" :with-time="false" @save="setDue" @cancel="openPicker = null" /></PickerFloat></div>
        <div class="field picker-field"><span class="field-label">{{ repeatTitle }}</span><div class="chips"><button v-for="item in repeatPresets" :key="item.id" :class="{ active: repeatSource === 'preset-' + item.id }" @click="applyRepeatPreset(item)">{{ item.name }}</button><button :class="{ active: repeatSource === 'custom', empty: repeatSource === 'empty' }" @click="togglePicker('repeat', $event)">自定义</button></div><PickerFloat v-if="openPicker === 'repeat' && pickerAnchor" :anchor="pickerAnchor" @close="onClosePicker"><RepeatPicker :value="item.repeat_rule" @save="setRepeat" @cancel="openPicker = null" /></PickerFloat></div>
      </template>
      <p v-else class="sub-hint">子任务不继承高级设置，也不能添加自己的子任务。</p>
    </div>
    <p v-else class="empty">选择或新建一个任务以开始编辑。</p>
  </section>
</template>

<style scoped>
.todo-lower { flex:1; overflow:auto; padding:14px 16px; box-sizing:border-box; background:rgba(255,255,255,.82); }.todo-title { margin:0 0 10px; padding-bottom:8px; border-bottom:1px solid rgba(0,0,0,.08); color:#333; font-size:14px; }.field { display:block; position:relative; padding:8px 0; color:#888; font-size:12.5px; }.field-label { display:block; margin-bottom:6px; }.field input,.field textarea { box-sizing:border-box; width:100%; border:1px solid rgba(0,0,0,.15); border-radius:6px; background:#fff; color:#333; padding:7px 8px; font-size:13px; font-family:inherit; resize:vertical; }.add-child-btn { margin:2px 0; padding:5px 8px; border:0; background:none; color:#4f7cff; font-size:12px; font-family:inherit; cursor:pointer; display:inline-flex; align-items:center; }
.add-child-btn .ri { vertical-align:-2px; margin-right:3px; }.chips { display:flex; flex-wrap:wrap; gap:6px; align-items:flex-start; }.chips button { min-width:56px; height:28px; box-sizing:border-box; padding:0 9px; display:inline-flex; align-items:center; justify-content:center; border:1px solid rgba(0,0,0,.12); border-radius:6px; background:rgba(255,255,255,.6); color:#444; font-size:12px; font-family:inherit; cursor:pointer; }.chips button:hover { background:#fff; border-color:rgba(0,0,0,.2); }.chips button.active { color:#fff; border-color:#4f7cff; background:#4f7cff; }.chips button.empty { margin:0; border-style:solid; border-color:rgba(0,0,0,.24); background:rgba(0,0,0,.05); color:#999; }.chips button.empty:hover { color:#4f7cff; }.sub-hint,.empty { margin:10px 0; color:#999; font-size:12px; }.empty { text-align:center; padding-top:30px; }
</style>
