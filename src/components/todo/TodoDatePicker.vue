<script setup lang="ts">
import { computed, ref, watch } from "vue";
import dayjs, { type Dayjs } from "dayjs";

const props = defineProps<{
  value: string | null;
  withTime: boolean;
}>();

const emit = defineEmits<{
  save: [value: string];
  cancel: [];
}>();

const selectedDay = ref<Dayjs>(dayjs());
const visibleMonth = ref<Dayjs>(dayjs().startOf("month"));
const hour = ref(9);
const minute = ref(0);

function reset(value: string | null) {
  const parsed = value ? dayjs(value) : dayjs();
  selectedDay.value = parsed.startOf("day");
  visibleMonth.value = parsed.startOf("month");
  hour.value = parsed.hour();
  minute.value = parsed.minute();
}

watch(() => props.value, reset, { immediate: true });

const years = computed(() => Array.from({ length: 11 }, (_, index) => dayjs().year() - 5 + index));
const weekdays = ["一", "二", "三", "四", "五", "六", "日"];
const days = computed(() => {
  const first = visibleMonth.value.startOf("month");
  const offset = (first.day() + 6) % 7;
  const start = first.subtract(offset, "day");
  return Array.from({ length: 42 }, (_, index) => start.add(index, "day"));
});

function changeMonth(delta: number) {
  visibleMonth.value = visibleMonth.value.add(delta, "month");
}

function changeYear(event: Event) {
  visibleMonth.value = visibleMonth.value.year(Number((event.target as HTMLSelectElement).value));
}

function changeMonthNumber(event: Event) {
  visibleMonth.value = visibleMonth.value.month(Number((event.target as HTMLSelectElement).value));
}

function save() {
  const next = selectedDay.value.hour(props.withTime ? hour.value : 0).minute(props.withTime ? minute.value : 0).second(0).millisecond(0);
  emit("save", next.toISOString());
}
</script>

<template>
  <div class="date-picker" @click.stop>
    <header class="picker-head">
      <div class="month-selects">
        <select :value="visibleMonth.year()" aria-label="年份" @change="changeYear">
          <option v-for="year in years" :key="year" :value="year">{{ year }} 年</option>
        </select>
        <select :value="visibleMonth.month()" aria-label="月份" @change="changeMonthNumber">
          <option v-for="month in 12" :key="month" :value="month - 1">{{ month }} 月</option>
        </select>
      </div>
      <div class="month-nav">
        <button title="上个月" @click="changeMonth(-1)"><i class="ri-arrow-left-s-line"></i></button>
        <button title="下个月" @click="changeMonth(1)"><i class="ri-arrow-right-s-line"></i></button>
      </div>
    </header>
    <div class="weekday-row">
      <span v-for="weekday in weekdays" :key="weekday">{{ weekday }}</span>
    </div>
    <div class="day-grid">
      <button
        v-for="day in days"
        :key="day.format('YYYY-MM-DD')"
        :class="{ dim: day.month() !== visibleMonth.month(), today: day.isSame(dayjs(), 'day'), selected: day.isSame(selectedDay, 'day') }"
        @click="selectedDay = day"
      >{{ day.date() }}</button>
    </div>
    <div v-if="withTime" class="time-row">
      <input v-model.number="hour" type="number" min="0" max="23" aria-label="小时" />
      <span>:</span>
      <input v-model.number="minute" type="number" min="0" max="59" aria-label="分钟" />
    </div>
    <footer class="picker-actions">
      <button class="cancel" @click="emit('cancel')">取消</button>
      <button class="save" @click="save">保存</button>
    </footer>
  </div>
</template>

<style scoped>
.date-picker { position:absolute; top:calc(100% + 6px); left:0; z-index:20; width:280px; box-sizing:border-box; padding:12px; border:1px solid rgba(0,0,0,.12); border-radius:10px; background:rgba(255,255,255,.98); box-shadow:0 8px 32px rgba(0,0,0,.22); color:#333; }
.picker-head,.month-selects,.month-nav,.time-row,.picker-actions { display:flex; align-items:center; }
.picker-head { justify-content:space-between; padding-bottom:8px; border-bottom:1px solid rgba(0,0,0,.08); }
.month-selects { gap:2px; }.month-selects select { border:0; border-radius:4px; background:transparent; color:#333; font:600 14px inherit; padding:3px 4px; cursor:pointer; }.month-selects select:hover { background:rgba(0,0,0,.04); }
.month-nav { gap:2px; }.month-nav button { width:24px; height:24px; border:0; border-radius:4px; background:transparent; color:#555; font-size:18px; line-height:1; display:inline-flex; align-items:center; justify-content:center; cursor:pointer; }.month-nav button:hover { background:rgba(79,124,255,.1); color:#4f7cff; }
.weekday-row,.day-grid { display:grid; grid-template-columns:repeat(7, 1fr); text-align:center; }.weekday-row { margin-top:8px; color:#888; font-size:11px; }.day-grid { gap:2px; margin-top:4px; }.day-grid button { aspect-ratio:1; border:0; border-radius:5px; background:transparent; color:#444; font:12px inherit; cursor:pointer; }.day-grid button:hover { background:rgba(79,124,255,.1); }.day-grid button.dim { color:#bbb; }.day-grid button.today { color:#4f7cff; box-shadow:inset 0 0 0 1px #4f7cff; }.day-grid button.selected { background:#4f7cff; color:#fff; box-shadow:none; font-weight:600; }
.time-row { justify-content:center; gap:7px; margin-top:9px; padding-top:9px; border-top:1px solid rgba(0,0,0,.08); }.time-row input { width:48px; border:1px solid rgba(0,0,0,.15); border-radius:6px; padding:4px; text-align:center; font:13px inherit; }
.picker-actions { justify-content:flex-end; gap:8px; margin-top:10px; padding-top:10px; border-top:1px solid rgba(0,0,0,.08); }.picker-actions button { border-radius:6px; padding:5px 12px; font:12.5px inherit; cursor:pointer; }.cancel { border:1px solid rgba(0,0,0,.15); background:#fff; color:#555; }.save { border:1px solid #4f7cff; background:#4f7cff; color:#fff; }.save:hover { background:#3b67e8; }
</style>
