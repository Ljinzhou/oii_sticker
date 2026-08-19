<script setup lang="ts">
import { ref, watch } from "vue";

interface RepeatRule {
  unit: "day" | "week" | "month" | "year";
  interval: number;
  weekdays?: number[];
}

const props = defineProps<{ value: string | null }>();
const emit = defineEmits<{ save: [value: string]; cancel: [] }>();
const rule = ref<RepeatRule>({ unit: "day", interval: 1 });
const weekdays = ["周日", "周一", "周二", "周三", "周四", "周五", "周六"];

function reset(value: string | null) {
  try {
    const candidate = JSON.parse(value ?? "{}") as Partial<RepeatRule>;
    rule.value = {
      unit: candidate.unit === "week" || candidate.unit === "month" || candidate.unit === "year" ? candidate.unit : "day",
      interval: Math.max(1, Number(candidate.interval) || 1),
      weekdays: Array.isArray(candidate.weekdays) ? candidate.weekdays.filter((day) => Number.isInteger(day) && day >= 0 && day <= 6) : [],
    };
  } catch {
    rule.value = { unit: "day", interval: 1 };
  }
}

watch(() => props.value, reset, { immediate: true });

function toggleWeekday(day: number) {
  const values = rule.value.weekdays ?? [];
  rule.value.weekdays = values.includes(day) ? values.filter((value) => value !== day) : [...values, day];
}

function save() {
  const value: RepeatRule = { unit: rule.value.unit, interval: Math.max(1, rule.value.interval) };
  if (value.unit === "week" && rule.value.weekdays?.length) value.weekdays = rule.value.weekdays;
  emit("save", JSON.stringify(value));
}
</script>

<template>
  <div class="repeat-picker" @click.stop>
    <strong class="picker-title">重复周期</strong>
    <div class="rule-row">
      <input v-model.number="rule.interval" type="number" min="1" max="99" aria-label="重复间隔" />
      <select v-model="rule.unit" aria-label="重复单位">
        <option value="day">天</option>
        <option value="week">周</option>
        <option value="month">月</option>
        <option value="year">年</option>
      </select>
    </div>
    <div v-if="rule.unit === 'week'" class="weekday-grid">
      <button v-for="(weekday, index) in weekdays" :key="weekday" :class="{ selected: rule.weekdays?.includes(index) }" @click="toggleWeekday(index)">{{ weekday }}</button>
    </div>
    <footer class="picker-actions">
      <button class="cancel" @click="emit('cancel')">取消</button>
      <button class="save" @click="save">保存</button>
    </footer>
  </div>
</template>

<style scoped>
.repeat-picker { position:absolute; top:calc(100% + 6px); left:0; z-index:20; width:280px; box-sizing:border-box; padding:12px; border:1px solid rgba(0,0,0,.12); border-radius:10px; background:rgba(255,255,255,.98); box-shadow:0 8px 32px rgba(0,0,0,.22); color:#333; }.picker-title { display:block; padding-bottom:9px; border-bottom:1px solid rgba(0,0,0,.08); font-size:13px; }.rule-row { display:flex; gap:8px; padding:10px 0; }.rule-row input { width:56px; box-sizing:border-box; border:1px solid rgba(0,0,0,.15); border-radius:6px; padding:6px; font:13px inherit; }.rule-row select { flex:1; border:1px solid rgba(0,0,0,.15); border-radius:6px; padding:6px; background:#fff; font:13px inherit; }.weekday-grid { display:grid; grid-template-columns:repeat(5, 1fr); gap:6px; padding-top:2px; }.weekday-grid button { border:0; border-radius:6px; padding:6px 3px; background:#f5f6f8; color:#555; font:12px inherit; cursor:pointer; }.weekday-grid button:hover { background:#eaeef5; }.weekday-grid button.selected { background:#4f7cff; color:#fff; font-weight:600; }.picker-actions { display:flex; justify-content:flex-end; gap:8px; margin-top:10px; padding-top:10px; border-top:1px solid rgba(0,0,0,.08); }.picker-actions button { border-radius:6px; padding:5px 12px; font:12.5px inherit; cursor:pointer; }.cancel { border:1px solid rgba(0,0,0,.15); background:#fff; color:#555; }.save { border:1px solid #4f7cff; background:#4f7cff; color:#fff; }.save:hover { background:#3b67e8; }
</style>
