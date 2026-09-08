<script setup lang="ts">
// 任务总览（主控台页面）：聚合当前工作空间所有便签 todo 块的任务，
// 支持筛选/搜索/分组展示、增删改查、提醒/截止/重复设置与提醒状态确认(ack)。
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { listen } from "../../composables/useTauri";
import { useNotesStore } from "../../stores/notes";
import { useSettingsStore } from "../../stores/settings";
import { useTodoOverviewStore } from "../../stores/todo-overview";
import { todoHighlightState, formatTodoDate, formatTodoRepeat } from "../../utils/todo-dates";
import { buildGroups, countStats, filterBlocks } from "../../utils/todo-overview";
import { parsePresetRule, presetToRepeatRule } from "../../utils/presets";
import type { TodoBlockWithSticker, TodoPatch } from "../../types";
import TodoDatePicker from "../todo/TodoDatePicker.vue";
import RepeatPicker from "../todo/RepeatPicker.vue";
import PickerFloat from "../todo/PickerFloat.vue";

const emit = defineEmits<{ "open-sticker": [stickerId: number] }>();

const notes = useNotesStore();
const settings = useSettingsStore();
const store = useTodoOverviewStore();

// ── 筛选与统计 ──
const filter = ref<{ today: boolean; week: boolean; alert: boolean; done: boolean; keyword: string }>({
  today: false,
  week: false,
  alert: false,
  done: false,
  keyword: "",
});
type FilterKey = "today" | "week" | "alert" | "done";
function setFilter(key: FilterKey) {
  filter.value[key] = !filter.value[key];
}
const stats = computed(() => countStats(store.blocks));
const filtered = computed(() =>
  filterBlocks(store.blocks, {
    today: filter.value.today,
    week: filter.value.week,
    alert: filter.value.alert,
    done: filter.value.done || undefined,
    keyword: filter.value.keyword,
  }),
);
const groups = computed(() => buildGroups(filtered.value, notes.stickers.map((s) => s.id)));
const counts = computed(() => {
  const all = store.blocks;
  return {
    today: filterBlocks(all, { today: true }).length,
    week: filterBlocks(all, { week: true }).length,
    alert: filterBlocks(all, { alert: true }).length,
    done: filterBlocks(all, { done: true }).length,
  };
});

// ── 分组折叠（会话级） ──
const collapsed = ref<Record<number, boolean>>({});
function toggleCollapse(stickerId: number) {
  collapsed.value[stickerId] = !collapsed.value[stickerId];
}

// ── 任务行状态 ──
/** 子任务判定：parent 指向另一条任务（块本身不在返回集合中）。 */
function isSubTask(item: TodoBlockWithSticker): boolean {
  return Boolean(item.parent_id && store.blocks.some((b) => b.id === item.parent_id));
}
function isReminded(item: TodoBlockWithSticker): boolean {
  return !item.is_completed && todoHighlightState(item).reminded;
}
function isOverdue(item: TodoBlockWithSticker): boolean {
  return !item.is_completed && todoHighlightState(item).overdue;
}

// ── 操作 ──
const toast = ref<string | null>(null);
let toastTimer: ReturnType<typeof setTimeout> | null = null;
function showToast(text: string) {
  if (toastTimer) clearTimeout(toastTimer);
  toast.value = text;
  toastTimer = setTimeout(() => (toast.value = null), 2800);
}
function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
async function onToggle(item: TodoBlockWithSticker, event: Event) {
  const checked = (event.target as HTMLInputElement).checked;
  try {
    await store.toggle(item.id, checked);
  } catch (error) {
    showToast(messageOf(error));
  }
}
async function onRemove(item: TodoBlockWithSticker) {
  try {
    await store.remove(item.id);
    showToast("任务已删除");
  } catch (error) {
    showToast(messageOf(error));
  }
}
async function onAck(item: TodoBlockWithSticker) {
  try {
    await store.ack(item.id);
    showToast("已确认提醒");
  } catch (error) {
    showToast(messageOf(error));
  }
}
async function onAddTask(stickerId: number) {
  try {
    const created = await store.create(stickerId);
    const fresh = store.blocks.find((b) => b.id === created.id);
    openEdit(fresh ?? (created as TodoBlockWithSticker));
  } catch (error) {
    showToast(messageOf(error));
  }
}

// ── 编辑浮层 ──
const editing = ref<TodoBlockWithSticker | null>(null);
const form = ref({ title: "", description: "", reminder_at: null as string | null, due_at: null as string | null, repeat_rule: null as string | null });
const openPicker = ref<"reminder" | "due" | "repeat" | null>(null);
const pickerAnchor = ref<DOMRect | null>(null);
let closedByTrigger: EventTarget | null = null;

function openEdit(item: TodoBlockWithSticker) {
  editing.value = item;
  form.value = {
    title: item.title ?? "",
    description: item.description ?? "",
    reminder_at: item.reminder_at,
    due_at: item.due_at,
    repeat_rule: item.repeat_rule,
  };
  openPicker.value = null;
}
function closeEdit() {
  editing.value = null;
  openPicker.value = null;
}
function togglePicker(kind: "reminder" | "due" | "repeat", event?: MouseEvent) {
  if (closedByTrigger && closedByTrigger === event?.currentTarget) {
    closedByTrigger = null;
    return;
  }
  closedByTrigger = null;
  if (openPicker.value === kind) {
    openPicker.value = null;
    return;
  }
  pickerAnchor.value = event?.currentTarget instanceof HTMLElement ? event.currentTarget.getBoundingClientRect() : null;
  openPicker.value = kind;
}
function onClosePicker(source?: Event) {
  closedByTrigger = source?.target instanceof Node ? source.target : null;
  openPicker.value = null;
}
function clearField(kind: "reminder" | "due" | "repeat") {
  if (kind === "reminder") form.value.reminder_at = null;
  else if (kind === "due") form.value.due_at = null;
  else form.value.repeat_rule = null;
  openPicker.value = null;
}
function saveReminder(value: string) {
  form.value.reminder_at = value;
  openPicker.value = null;
}
function saveDue(value: string) {
  form.value.due_at = value;
  openPicker.value = null;
}
function saveRepeat(value: string) {
  form.value.repeat_rule = value;
  openPicker.value = null;
}
function resetFilter() {
  filter.value = { today: false, week: false, alert: false, done: false, keyword: filter.value.keyword };
}
// 预设 chips（系统设置 → Todo 设置）
const reminderPresets = computed(() => settings.todoPresets.reminders);
const duePresets = computed(() => settings.todoPresets.due);
const repeatPresets = computed(() => settings.todoPresets.repeats);
function applyReminderPreset(rule: Parameters<typeof parsePresetRule>[0]) {
  const iso = parsePresetRule(rule);
  if (iso) form.value.reminder_at = iso;
}
function applyDuePreset(rule: Parameters<typeof parsePresetRule>[0]) {
  const iso = parsePresetRule(rule);
  if (iso) form.value.due_at = iso;
}
function applyRepeatPreset(rule: Parameters<typeof presetToRepeatRule>[0]) {
  const value = presetToRepeatRule(rule);
  if (value) form.value.repeat_rule = value;
}
async function saveEdit() {
  const item = editing.value;
  if (!item) return;
  const patch: TodoPatch = {
    title: form.value.title,
    description: form.value.description,
    // 空串语义与 TodoDetail 一致："" = 清空该字段（后端据此移除）
    reminder_at: form.value.reminder_at ?? "",
    due_at: form.value.due_at ?? "",
    repeat_rule: form.value.repeat_rule ?? "",
  };
  try {
    await store.patch(item.id, patch);
    showToast("任务已保存");
    closeEdit();
  } catch (error) {
    showToast(messageOf(error));
  }
}
const editingIsSubTask = computed(() => (editing.value ? isSubTask(editing.value) : false));

// ── 事件监听：todo 变更/提醒触发 → 全量刷新 ──
const unlisteners: UnlistenFn[] = [];
onMounted(async () => {
  await store.load();
  unlisteners.push(await listen("todo://updated", () => void store.load()));
  unlisteners.push(await listen("todo://reminder-fired", () => void store.load()));
  // 便签删除等变更也会间接影响任务列表（FK 级联）
  unlisteners.push(await listen("sticky://push-update", () => void store.load()));
});
onBeforeUnmount(() => {
  unlisteners.forEach((u) => u());
  if (toastTimer) clearTimeout(toastTimer);
});
</script>

<template>
  <section class="todo-overview">
    <!-- 统计条 -->
    <div class="ov-stats">
      <span class="ov-stat">共 <b>{{ stats.total }}</b> 项任务</span>
      <span class="ov-stat">待办 <b>{{ stats.pending }}</b></span>
      <span class="ov-stat warn" :class="{ on: filter.alert }">
        <i class="ri-notification-3-line"></i>提醒/逾期 <b>{{ stats.reminded + stats.overdue }}</b>
      </span>
      <span class="ov-stat done">已完成 <b>{{ stats.done }}</b></span>
    </div>

    <!-- 工具条：筛选 chips + 搜索 -->
    <div class="ov-toolbar">
      <div class="chips">
        <button :class="{ on: !filter.today && !filter.week && !filter.alert && !filter.done }" @click="resetFilter">
          全部 <span class="count">{{ stats.total }}</span>
        </button>
        <button :class="{ on: filter.today }" @click="setFilter('today')">今天到期 <span class="count">{{ counts.today }}</span></button>
        <button :class="{ on: filter.week }" @click="setFilter('week')">未来 7 天 <span class="count">{{ counts.week }}</span></button>
        <button :class="{ on: filter.done }" @click="setFilter('done')">已完成 <span class="count">{{ counts.done }}</span></button>
      </div>
      <div class="ov-search">
        <i class="ri-search-line"></i>
        <input v-model="filter.keyword" placeholder="搜索任务…" />
        <button v-if="filter.keyword" class="ov-clear" title="清空" @click="filter.keyword = ''"><i class="ri-close-circle-fill"></i></button>
      </div>
    </div>

    <!-- 按便签分组卡片 -->
    <div v-if="store.loading && groups.length === 0" class="ov-loading">加载中…</div>
    <template v-else>
      <div v-for="group in groups" :key="group.stickerId" class="ov-group">
        <header class="ov-group-head" @click="toggleCollapse(group.stickerId)">
          <span class="ov-caret"><i :class="collapsed[group.stickerId] ? 'ri-arrow-right-s-line' : 'ri-arrow-down-s-line'"></i></span>
          <i class="ri-sticky-note-fill ov-g-icon"></i>
          <span class="ov-group-name">{{ group.stickerTitle }}</span>
          <span class="ov-group-count">{{ group.items.length }} 项</span>
          <button class="ov-open-sticker" title="打开便签" @click.stop="emit('open-sticker', group.stickerId)">
            <i class="ri-external-link-line"></i>打开便签
          </button>
        </header>
        <div v-show="!collapsed[group.stickerId]" class="ov-rows">
          <div
            v-for="item in group.items"
            :key="item.id"
            class="ov-row"
            :class="{ done: item.is_completed, reminded: isReminded(item), overdue: isOverdue(item), sub: isSubTask(item) }"
            @click="openEdit(item)"
          >
            <input
              type="checkbox"
              class="wb"
              :checked="item.is_completed"
              @click.stop
              @change="onToggle(item, $event)"
            />
            <span class="ov-lbl">
              {{ item.title || "未命名任务" }}
              <span v-if="isSubTask(item)" class="ov-sub-tag">子任务</span>
            </span>
            <span class="ov-badges">
              <span v-if="item.reminder_at" class="hl time"><i class="ri-time-line"></i>提醒 {{ formatTodoDate(item.reminder_at) }}</span>
              <span v-if="item.due_at" class="hl time"><i class="ri-calendar-line"></i>截止 {{ formatTodoDate(item.due_at) }}</span>
              <span v-if="item.repeat_rule" class="hl todo"><i class="ri-restart-line"></i>{{ formatTodoRepeat(item.repeat_rule) }}</span>
            </span>
            <button v-if="isReminded(item) || isOverdue(item)" class="ov-ack" title="确认收到提醒，不再高亮" @click.stop="onAck(item)">
              <i class="ri-check-double-line"></i>已确认
            </button>
            <button class="ov-del" title="删除任务" @click.stop="onRemove(item)"><i class="ri-close-line"></i></button>
          </div>
          <button class="ov-add-task" @click="onAddTask(group.stickerId)"><i class="ri-add-line"></i>添加任务</button>
        </div>
      </div>

      <p v-if="groups.length === 0 && !store.loading" class="ov-empty">
        <i class="ri-todo-line"></i>
        {{ store.blocks.length === 0 ? "还没有任务：在便签中用「/todo 块」创建任务后，会在这里汇总显示" : "没有符合条件的任务" }}
      </p>
    </template>

    <!-- 编辑浮层 -->
    <div v-if="editing" class="ov-mask" @click.self="closeEdit" @keydown.esc="closeEdit">
      <div class="ov-sheet">
        <h3>
          {{ editingIsSubTask ? "编辑子任务" : "编辑任务" }}
          <button class="ov-x" @click="closeEdit"><i class="ri-close-line"></i></button>
        </h3>
        <div class="ov-field">
          <label>任务标题</label>
          <input v-model="form.title" type="text" placeholder="输入任务名称" />
        </div>
        <div class="ov-field">
          <label>任务描述</label>
          <textarea v-model="form.description" rows="2" placeholder="补充说明（可选）"></textarea>
        </div>
        <div class="ov-field ov-src">
          <label>所属便签</label>
          <span class="ov-src-val"><i class="ri-sticky-note-line"></i>{{ editing.sticker_title }}</span>
        </div>
        <template v-if="!editingIsSubTask">
          <div class="ov-field ov-picker-field">
            <label>提醒时间 · {{ form.reminder_at ? formatTodoDate(form.reminder_at) : "未设置" }}</label>
            <div class="ov-chips">
              <button v-for="p in reminderPresets" :key="p.id" @click="applyReminderPreset(p.rule)">{{ p.name }}</button>
              <button :class="{ on: openPicker === 'reminder' }" @click="togglePicker('reminder', $event)">自定义</button>
              <button v-if="form.reminder_at" class="ov-clear-field" @click="clearField('reminder')">清除</button>
            </div>
            <PickerFloat v-if="openPicker === 'reminder' && pickerAnchor" :anchor="pickerAnchor" @close="onClosePicker">
              <TodoDatePicker :value="form.reminder_at" with-time @save="saveReminder" @cancel="openPicker = null" />
            </PickerFloat>
          </div>
          <div class="ov-field ov-picker-field">
            <label>截止时间 · {{ form.due_at ? formatTodoDate(form.due_at) : "未设置" }}</label>
            <div class="ov-chips">
              <button v-for="p in duePresets" :key="p.id" @click="applyDuePreset(p.rule)">{{ p.name }}</button>
              <button :class="{ on: openPicker === 'due' }" @click="togglePicker('due', $event)">自定义</button>
              <button v-if="form.due_at" class="ov-clear-field" @click="clearField('due')">清除</button>
            </div>
            <PickerFloat v-if="openPicker === 'due' && pickerAnchor" :anchor="pickerAnchor" @close="onClosePicker">
              <TodoDatePicker :value="form.due_at" :with-time="false" @save="saveDue" @cancel="openPicker = null" />
            </PickerFloat>
          </div>
          <div class="ov-field ov-picker-field">
            <label>重复规则 · {{ form.repeat_rule ? formatTodoRepeat(form.repeat_rule) : "不重复" }}</label>
            <div class="ov-chips">
              <button v-for="p in repeatPresets" :key="p.id" @click="applyRepeatPreset(p.rule)">{{ p.name }}</button>
              <button :class="{ on: openPicker === 'repeat' }" @click="togglePicker('repeat', $event)">自定义</button>
              <button v-if="form.repeat_rule" class="ov-clear-field" @click="clearField('repeat')">清除</button>
            </div>
            <PickerFloat v-if="openPicker === 'repeat' && pickerAnchor" :anchor="pickerAnchor" @close="onClosePicker">
              <RepeatPicker :value="form.repeat_rule" @save="saveRepeat" @cancel="openPicker = null" />
            </PickerFloat>
          </div>
        </template>
        <p v-else class="ov-sub-hint">子任务只支持名称与描述，不继承提醒/截止/重复设置。</p>
        <div class="ov-actions">
          <button class="btn" @click="closeEdit">取消</button>
          <button class="btn primary" @click="saveEdit"><i class="ri-check-line"></i>保存</button>
        </div>
      </div>
    </div>

    <div v-if="toast" class="ov-toast">{{ toast }}</div>
  </section>
</template>

<style scoped>
.todo-overview {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

/* 统计条 */
.ov-stats {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
}
.ov-stat {
  background: #fff;
  border: 1px solid rgba(0, 0, 0, 0.08);
  border-radius: 8px;
  padding: 5px 11px;
  font-size: 11.5px;
  color: #666;
  display: inline-flex;
  gap: 5px;
  align-items: center;
}
.ov-stat b {
  color: #333;
  font-size: 13px;
}
.ov-stat.warn b {
  color: #a35b04;
}
.ov-stat.done b {
  color: #2f9e5f;
}

/* 工具条 */
.ov-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  flex-wrap: wrap;
}
.chips {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}
.chips button {
  border: 1px solid rgba(0, 0, 0, 0.1);
  border-radius: 999px;
  padding: 4px 12px;
  font-size: 11.5px;
  background: #fff;
  color: #555;
  cursor: pointer;
  font: inherit;
  transition: background 0.15s, color 0.15s;
}
.chips button:hover {
  background: #f2f4f7;
}
.chips button.on {
  background: #4f7cff;
  border-color: #4f7cff;
  color: #fff;
}
.chips .count {
  opacity: 0.75;
  font-size: 10.5px;
  margin-left: 2px;
}
.ov-search {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  background: #fff;
  border: 1px solid rgba(0, 0, 0, 0.12);
  border-radius: 8px;
  padding: 5px 9px;
  color: #aaa;
}
.ov-search input {
  border: none;
  outline: none;
  font-size: 12px;
  font-family: inherit;
  width: 120px;
  color: #333;
  background: transparent;
}
.ov-clear {
  border: 0;
  background: transparent;
  color: #bbb;
  cursor: pointer;
  font-size: 13px;
  display: inline-flex;
  padding: 0;
}
.ov-clear:hover {
  color: #4f7cff;
}

/* 分组卡片 */
.ov-group {
  margin-bottom: 10px;
}
.ov-group-head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 7px 12px;
  background: #fbf7ec;
  border-radius: 8px;
  cursor: pointer;
  user-select: none;
  transition: background 0.15s;
}
.ov-group-head:hover {
  background: #f5efe0;
}
.ov-caret {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  font-size: 16px;
  line-height: 1;
  color: #998a66;
  width: 18px;
  flex: none;
}
.ov-g-icon {
  color: #998a66;
  font-size: 14px;
}
.ov-group-name {
  font-size: 13px;
  font-weight: 600;
  color: #444;
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.ov-group-count {
  flex: none;
  font-size: 11px;
  line-height: 1;
  padding: 4px 8px;
  border-radius: 999px;
  background: rgba(79, 124, 255, 0.12);
  color: #3b67e8;
}
.ov-open-sticker {
  flex: none;
  border: 0;
  background: transparent;
  color: #998a66;
  font-size: 11.5px;
  cursor: pointer;
  padding: 3px 7px;
  border-radius: 6px;
  display: inline-flex;
  align-items: center;
  gap: 3px;
  font-family: inherit;
}
.ov-open-sticker:hover {
  background: #eae2cf;
  color: #6b5d3e;
}

/* 任务行 */
.ov-rows {
  margin-top: 6px;
}
.ov-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 9px;
  border-radius: 7px;
  background: #fff;
  border: 1px solid rgba(0, 0, 0, 0.06);
  margin-bottom: 4px;
  font-size: 12.5px;
  cursor: pointer;
  transition: background 0.15s;
}
.ov-row:hover {
  border-color: rgba(79, 124, 255, 0.35);
}
.ov-row.reminded {
  background: rgba(255, 213, 145, 0.35);
  box-shadow: inset 3px 0 #f08a14;
  animation: ov-remind-pulse 2s ease-in-out infinite;
}
.ov-row.reminded .ov-lbl {
  color: #8a5200;
}
.ov-row.overdue {
  background: rgba(255, 205, 198, 0.45);
  box-shadow: inset 3px 0 #b42318;
}
.ov-row.overdue .ov-lbl {
  color: #b42318;
}
@keyframes ov-remind-pulse {
  0%,
  100% {
    box-shadow: inset 3px 0 #f08a14;
  }
  50% {
    box-shadow: inset 3px 0 #f08a14, 0 0 0 2px rgba(240, 138, 20, 0.18);
  }
}
.ov-lbl {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
  color: #333;
}
.ov-row.done .ov-lbl {
  color: #999;
  text-decoration: line-through;
}
.ov-sub-tag {
  margin-left: 6px;
  font-size: 10px;
  color: #8a9a6a;
  border: 1px solid rgba(0, 0, 0, 0.1);
  border-radius: 999px;
  padding: 1px 6px;
  vertical-align: 1px;
  white-space: nowrap;
}
.ov-badges {
  display: inline-flex;
  gap: 5px;
  flex: none;
  overflow: hidden;
}
.hl {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  font-size: 10.5px;
  line-height: 1;
  padding: 2px 7px;
  border-radius: 999px;
  white-space: nowrap;
}
.hl.time {
  background: rgba(0, 0, 0, 0.05);
  color: #777;
}
.hl.todo {
  background: rgba(79, 124, 255, 0.12);
  color: #3b67e8;
}
.ov-ack {
  flex: none;
  border: 1px solid rgba(240, 138, 20, 0.5);
  background: rgba(240, 138, 20, 0.08);
  color: #a35b04;
  font-size: 11px;
  padding: 3px 9px;
  border-radius: 999px;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  gap: 3px;
  font-family: inherit;
  white-space: nowrap;
}
.ov-ack:hover {
  background: rgba(240, 138, 20, 0.18);
}
.ov-del {
  flex: none;
  width: 20px;
  height: 20px;
  border: 0;
  border-radius: 5px;
  background: transparent;
  color: #bbb;
  font-size: 14px;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  visibility: hidden;
}
.ov-row:hover .ov-del {
  visibility: visible;
}
.ov-del:hover {
  color: #d33;
  background: #ffe3e3;
}
.ov-add-task {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  border: 1px dashed rgba(79, 124, 255, 0.35);
  background: rgba(79, 124, 255, 0.04);
  color: #4f7cff;
  font-size: 12px;
  font-family: inherit;
  padding: 5px 12px;
  border-radius: 7px;
  cursor: pointer;
  margin-top: 2px;
}
.ov-add-task:hover {
  background: rgba(79, 124, 255, 0.1);
}
.ov-loading,
.ov-empty {
  color: #b9b2a2;
  font-size: 12.5px;
  text-align: center;
  padding: 36px 0 26px;
}
.ov-empty .ri {
  font-size: 30px;
  display: block;
  margin-bottom: 8px;
  color: #d8d0ba;
}

/* 编辑浮层 */
.ov-mask {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.3);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 60;
}
.ov-sheet {
  width: 360px;
  max-height: 88vh;
  overflow: auto;
  background: #fff;
  border-radius: 12px;
  box-shadow: 0 14px 44px rgba(0, 0, 0, 0.3);
  padding: 16px 18px;
}
.ov-sheet h3 {
  margin: 0 0 12px;
  font-size: 14.5px;
  color: #333;
  display: flex;
  justify-content: space-between;
  align-items: center;
}
.ov-x {
  border: 0;
  background: transparent;
  color: #999;
  font-size: 16px;
  cursor: pointer;
}
.ov-x:hover {
  color: #333;
}
.ov-field {
  margin-bottom: 10px;
}
.ov-field label {
  display: block;
  font-size: 11.5px;
  color: #666;
  margin-bottom: 4px;
}
.ov-field input[type="text"],
.ov-field textarea {
  width: 100%;
  border: 1px solid rgba(0, 0, 0, 0.14);
  border-radius: 8px;
  padding: 6px 9px;
  font-size: 12.5px;
  font-family: inherit;
  outline: none;
  color: #333;
  box-sizing: border-box;
}
.ov-field input:focus,
.ov-field textarea:focus {
  border-color: #4f7cff;
}
.ov-field textarea {
  resize: vertical;
}
.ov-src-val {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 12.5px;
  color: #666;
}
.ov-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 5px;
}
.ov-chips button {
  border: 1px solid rgba(0, 0, 0, 0.12);
  border-radius: 6px;
  background: rgba(255, 255, 255, 0.7);
  color: #444;
  font-size: 11.5px;
  font-family: inherit;
  padding: 3px 9px;
  cursor: pointer;
}
.ov-chips button:hover {
  background: #fff;
  border-color: rgba(0, 0, 0, 0.2);
}
.ov-chips button.on {
  color: #fff;
  border-color: #4f7cff;
  background: #4f7cff;
}
.ov-chips .ov-clear-field {
  color: #d33;
}
.ov-sub-hint {
  margin: 8px 0;
  color: #999;
  font-size: 12px;
}
.ov-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 14px;
}

/* toast */
.ov-toast {
  position: absolute;
  bottom: 18px;
  left: 50%;
  transform: translateX(-50%);
  background: rgba(34, 34, 34, 0.92);
  color: #fff;
  font-size: 12.5px;
  padding: 7px 14px;
  border-radius: 8px;
  box-shadow: 0 6px 20px rgba(0, 0, 0, 0.25);
  z-index: 70;
  white-space: nowrap;
}
</style>