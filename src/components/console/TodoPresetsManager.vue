<script setup lang="ts">
// Todo 预设管理：一组预设列表 + 「添加预设」弹窗（名称 + 四类规则编辑器）。
// 提醒/截止走 相对/日历/星期 三类；重复走 周期 类。保存即落库 system_config。
import { computed, onMounted, reactive, ref } from "vue";
import { useSettingsStore } from "../../stores/settings";
import {
  describePresetRule,
  makePresetItem,
  sanitizeRule,
  type PresetItem,
  type PresetKind,
  type PresetRule,
} from "../../utils/presets";

const props = defineProps<{
  kind: PresetKind;
  title: string;
  hint?: string;
}>();

const settings = useSettingsStore();
const items = computed<PresetItem[]>(() => settings.todoPresets[props.kind]);

// 设置页打开时拉取最新配置（出厂缺省在 store getter 中兜底）
onMounted(() => {
  void settings.refresh();
});

const WEEKDAYS = ["周日", "周一", "周二", "周三", "周四", "周五", "周六"];
const UNITS: Record<string, string> = { h: "小时", d: "天", w: "周" };
const CYCLE_UNITS: Record<string, string> = { day: "天", week: "周", month: "月", year: "年" };

// ── 模态状态 ──
const showModal = ref(false);
const editingId = ref<string | null>(null);
const activeTab = ref<"relative" | "calendar" | "weekday" | "cycle">("relative");
const form = reactive({
  name: "",
  // relative
  relN: 1,
  relUnit: "h" as "h" | "d" | "w",
  relTime: "",
  // calendar
  calDate: "",
  calTime: "09:00",
  // weekday
  wdDays: [] as number[],
  wdInterval: 1,
  wdTime: "09:00",
  // cycle
  cycInterval: 1,
  cycUnit: "day" as "day" | "week" | "month" | "year",
  cycDays: [] as number[],
});

/** tabs：提醒/截止不含「周期」，重复只含「周期」。 */
const tabs = computed<{ key: typeof activeTab.value; label: string }[]>(() => {
  if (props.kind === "repeats") return [{ key: "cycle", label: "重复周期" }];
  return [
    { key: "relative", label: "相对时间" },
    { key: "calendar", label: "日历时间" },
    { key: "weekday", label: "星期规则" },
  ];
});

/** 当前 tab 表单 → 规则对象（未校验，保存时 sanitize）。 */
function ruleFromForm(): PresetRule {
  switch (activeTab.value) {
    case "relative":
      return { kind: "relative", n: form.relN, unit: form.relUnit, time: form.relTime || null };
    case "calendar":
      return { kind: "calendar", date: form.calDate, time: form.calTime };
    case "weekday":
      return { kind: "weekday", weekdays: [...form.wdDays], interval: form.wdInterval, time: form.wdTime };
    case "cycle":
      return {
        kind: "cycle",
        interval: form.cycInterval,
        unit: form.cycUnit,
        weekdays: form.cycUnit === "week" && form.cycDays.length ? [...form.cycDays] : null,
      };
  }
}

const preview = computed(() => {
  const rule = sanitizeRule(ruleFromForm());
  return rule ? describePresetRule(rule) : "";
});

const canSave = computed(() => {
  const rule = sanitizeRule(ruleFromForm());
  if (!rule) return false;
  if (props.kind === "repeats") return rule.kind === "cycle";
  return rule.kind !== "cycle";
});

function openAdd() {
  editingId.value = null;
  form.name = "";
  activeTab.value = tabs.value[0].key;
  resetForm();
  showModal.value = true;
}

function openEdit(item: PresetItem) {
  editingId.value = item.id;
  form.name = item.name;
  const rule = item.rule;
  activeTab.value = rule.kind === "cycle" ? "cycle" : (rule.kind as typeof activeTab.value);
  resetForm();
  switch (rule.kind) {
    case "relative":
      form.relN = rule.n;
      form.relUnit = rule.unit;
      form.relTime = rule.time ?? "";
      break;
    case "calendar":
      form.calDate = rule.date;
      form.calTime = rule.time;
      break;
    case "weekday":
      form.wdDays = [...rule.weekdays];
      form.wdInterval = rule.interval;
      form.wdTime = rule.time;
      break;
    case "cycle":
      form.cycInterval = rule.interval;
      form.cycUnit = rule.unit;
      form.cycDays = rule.weekdays ? [...rule.weekdays] : [];
      break;
  }
  showModal.value = true;
}

function resetForm() {
  form.relN = 1;
  form.relUnit = "h";
  form.relTime = "";
  form.calDate = "";
  form.calTime = "09:00";
  form.wdDays = [];
  form.wdInterval = 1;
  form.wdTime = "09:00";
  form.cycInterval = 1;
  form.cycUnit = "day";
  form.cycDays = [];
}

function toggleDay(target: number[], day: number) {
  return target.includes(day) ? target.filter((d) => d !== day) : [...target, day];
}

async function save() {
  const rule = sanitizeRule(ruleFromForm());
  if (!rule || !canSave.value) return;
  let list = [...items.value];
  if (editingId.value) {
    list = list.map((item) => (item.id === editingId.value ? { ...item, name: form.name.trim() || describePresetRule(rule), rule } : item));
  } else {
    list = [...list, makePresetItem(form.name, rule, props.kind)];
  }
  await settings.setTodoPresets(props.kind, list);
  showModal.value = false;
}

async function remove(id: string) {
  await settings.setTodoPresets(props.kind, items.value.filter((item) => item.id !== id));
}
</script>

<template>
  <section class="preset-section">
    <h3>{{ title }} <small v-if="hint">{{ hint }}</small></h3>
    <ul class="preset-list">
      <li v-for="item in items" :key="item.id">
        <span class="preset-name">{{ item.name || "未命名" }}</span>
        <span class="preset-rule">{{ describePresetRule(item.rule) }}</span>
        <span class="preset-badge">{{ item.rule.kind === "cycle" ? "周期" : item.rule.kind === "weekday" ? "星期" : item.rule.kind === "relative" ? "相对" : "日历" }}</span>
        <button class="icon-btn" title="编辑" @click="openEdit(item)">✎</button>
        <button class="icon-btn del" title="删除" @click="remove(item.id)">✕</button>
      </li>
      <li v-if="items.length === 0" class="empty-row">暂无预设，点击下方按钮添加。</li>
    </ul>
    <button class="add-btn" @click="openAdd">＋ 添加{{ title.replace("预设", "") }}预设</button>

    <!-- 添加/编辑弹窗 -->
    <div v-if="showModal" class="overlay" @click.self="showModal = false">
      <div class="modal">
        <h2>{{ editingId ? "编辑预设" : "添加预设" }}</h2>
        <div class="field">
          <label>预设名称（显示在 Todo 详情按钮上）</label>
          <input v-model="form.name" type="text" placeholder="例如：1小时后 / 下班后 / 周五下班前" />
        </div>
        <div class="field">
          <label>设定日期与时间</label>
          <div class="rule-tabs">
            <button v-for="tab in tabs" :key="tab.key" :class="{ on: activeTab === tab.key }" @click="activeTab = tab.key">
              {{ tab.label }}
            </button>
          </div>

          <div v-if="activeTab === 'relative'" class="rule-pane">
            <div class="row-grid">
              <div class="num">＋<input v-model.number="form.relN" type="number" min="0" max="3650" />
                <select v-model="form.relUnit">
                  <option v-for="(name, value) in UNITS" :key="value" :value="value">{{ name }}</option>
                </select>
              </div>
              <div><label class="mini">（可选）固定到时刻</label><input v-model="form.relTime" type="time" /></div>
            </div>
          </div>

          <div v-else-if="activeTab === 'calendar'" class="rule-pane">
            <div class="row-grid">
              <div><label class="mini">日期</label><input v-model="form.calDate" type="date" /></div>
              <div><label class="mini">时刻</label><input v-model="form.calTime" type="time" /></div>
            </div>
          </div>

          <div v-else-if="activeTab === 'weekday'" class="rule-pane">
            <div class="weekday-grid">
              <button v-for="(name, day) in WEEKDAYS" :key="day" :class="{ selected: form.wdDays.includes(day) }" @click="form.wdDays = toggleDay(form.wdDays, day)">
                {{ name }}
              </button>
            </div>
            <div class="row-grid mt">
              <div class="num2">每 <input v-model.number="form.wdInterval" type="number" min="1" max="52" /> 周</div>
              <div class="grow"><input v-model="form.wdTime" type="time" /></div>
            </div>
            <p class="tip">选择「每周几」即解析为下一次匹配的星期时刻。</p>
          </div>

          <div v-else class="rule-pane">
            <div class="row-grid">
              <div class="num2">每 <input v-model.number="form.cycInterval" type="number" min="1" max="99" />
                <select v-model="form.cycUnit">
                  <option v-for="(name, value) in CYCLE_UNITS" :key="value" :value="value">{{ name }}</option>
                </select>
              </div>
            </div>
            <div v-if="form.cycUnit === 'week'" class="weekday-grid mt">
              <button v-for="(name, day) in WEEKDAYS" :key="day" :class="{ selected: form.cycDays.includes(day) }" @click="form.cycDays = toggleDay(form.cycDays, day)">
                {{ name }}
              </button>
            </div>
            <p class="tip">重复周期驱动每日重建：到期未完成的任务次日自动改名「——YYYY年M月D日，任务逾期」并新建同名任务。</p>
          </div>
        </div>

        <p v-if="preview" class="preview">规则预览：{{ preview }}</p>

        <div class="modal-actions">
          <button class="btn-cancel" @click="showModal = false">取消</button>
          <button class="btn-save" :disabled="!canSave" @click="save">保存</button>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.preset-section { margin-top: 18px; }
.preset-section h3 { font-size: 14px; display: flex; align-items: baseline; gap: 8px; }
.preset-section h3 small { color: #888; font-size: 11.5px; font-weight: normal; }
.preset-list { list-style: none; margin: 8px 0 0; padding: 0; border: 1px solid rgba(0,0,0,.1); border-radius: 10px; background: #fff; overflow: hidden; }
.preset-list li { display: flex; align-items: center; gap: 10px; padding: 8px 12px; border-bottom: 1px solid rgba(0,0,0,.1); font-size: 13px; }
.preset-list li:last-child { border-bottom: 0; }
.preset-list li.empty-row { color: #999; justify-content: center; padding: 14px; }
.preset-name { flex: none; min-width: 90px; font-weight: 600; }
.preset-rule { flex: 1; color: #777; font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.preset-badge { flex: none; font-size: 11px; color: #4f7cff; border: 1px solid rgba(79,124,255,.4); border-radius: 999px; padding: 1px 8px; background: rgba(79,124,255,.06); }
.icon-btn { border: 0; background: none; color: #aaa; font-size: 14px; cursor: pointer; width: 24px; height: 24px; border-radius: 6px; flex: none; }
.icon-btn:hover { background: #f0f2f6; color: #333; }
.icon-btn.del:hover { background: #ffe9e9; color: #d33; }
.add-btn { margin-top: 8px; border: 1px dashed rgba(79,124,255,.55); background: rgba(79,124,255,.05); color: #4f7cff; width: 100%; padding: 8px; border-radius: 8px; cursor: pointer; font-size: 12.5px; }
.add-btn:hover { background: rgba(79,124,255,.12); }

.overlay { position: fixed; inset: 0; background: rgba(0,0,0,.32); display: flex; align-items: center; justify-content: center; z-index: 60; }
.modal { width: 440px; max-width: 92vw; max-height: 86vh; overflow: auto; background: #fff; border-radius: 14px; box-shadow: 0 18px 50px rgba(0,0,0,.25); padding: 20px 22px; }
.modal h2 { font-size: 15px; margin: 0 0 14px; }
.field { margin-bottom: 12px; }
.field > label { display: block; font-size: 12.5px; color: #777; margin-bottom: 6px; }
.field input[type="text"], .field input[type="number"] { width: 100%; border: 1px solid rgba(0,0,0,.16); border-radius: 8px; padding: 8px 10px; font-size: 13px; font-family: inherit; box-sizing: border-box; }
.rule-tabs { display: flex; gap: 6px; flex-wrap: wrap; margin-bottom: 10px; }
.rule-tabs button { border: 1px solid rgba(0,0,0,.1); background: #f7f8fa; border-radius: 8px; padding: 5px 10px; font-size: 12px; cursor: pointer; color: #333; }
.rule-tabs button.on { border-color: #4f7cff; color: #fff; background: #4f7cff; }
.row-grid { display: flex; gap: 8px; align-items: flex-end; flex-wrap: wrap; }
.row-grid .grow { flex: 1; }
.row-grid .num { display: flex; gap: 6px; align-items: center; }
.row-grid .num input, .row-grid .num2 input { width: 62px; border: 1px solid rgba(0,0,0,.16); border-radius: 8px; padding: 7px 8px; font-size: 13px; font-family: inherit; }
.row-grid.select-unit select, select { border: 1px solid rgba(0,0,0,.16); border-radius: 8px; padding: 7px 8px; font-size: 13px; font-family: inherit; background: #fff; }
input[type="date"], input[type="time"] { border: 1px solid rgba(0,0,0,.16); border-radius: 8px; padding: 7px 8px; font-size: 13px; font-family: inherit; background: #fff; }
.mini { display: block; font-size: 11.5px; color: #888; margin-bottom: 3px; }
.mt { margin-top: 8px; }
.weekday-grid { display: grid; grid-template-columns: repeat(7, 1fr); gap: 5px; }
.weekday-grid button { border: 1px solid rgba(0,0,0,.1); border-radius: 6px; padding: 6px 0; background: #f7f8fa; font-size: 12px; cursor: pointer; color: #555; }
.weekday-grid button.selected { background: #4f7cff; color: #fff; border-color: #4f7cff; }
.tip { margin: 8px 0 0; color: #999; font-size: 11.5px; line-height: 1.6; }
.preview { margin: 10px 0 0; color: #4f7cff; font-size: 12px; background: rgba(79,124,255,.07); border-radius: 8px; padding: 7px 10px; }
.modal-actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 16px; }
.modal-actions button { border-radius: 8px; padding: 7px 16px; font-size: 13px; font-family: inherit; cursor: pointer; }
.btn-cancel { border: 1px solid rgba(0,0,0,.1); background: #fff; color: #555; }
.btn-save { border: 1px solid #4f7cff; background: #4f7cff; color: #fff; }
.btn-save:disabled { opacity: .45; cursor: not-allowed; }
</style>