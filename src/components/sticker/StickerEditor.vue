<script setup lang="ts">
import { ref, computed, onMounted } from "vue";
import { invoke } from "../../composables/useTauri";
import { useSettingsStore } from "../../stores/settings";

const props = defineProps<{
  content: string;
  stickerId: number;
}>();

const emit = defineEmits<{
  saved: [];
  cancelled: [];
}>();

const settings = useSettingsStore();
const draft = ref(props.content);
const textarea = ref<HTMLTextAreaElement | null>(null);

// 编辑模式下文字字号（system_config edit_font_size，默认 14）
const editFontSize = computed(() => settings.get("edit_font_size", "14"));

/** 从 markdown 第一行提取标题（`# xxx` → xxx）。 */
function extractTitle(text: string): string {
  const first = text.split("\n").find((l) => l.trim().length > 0) ?? "";
  const trimmed = first.trim();
  if (trimmed.startsWith("# ")) {
    return trimmed.slice(2).trim();
  }
  return trimmed.slice(0, 30);
}

/** 保存：Markdown 原文直接落库，退出后进入交互模式才渲染。 */
async function save() {
  const title = extractTitle(draft.value);
  try {
    await invoke("update_sticker_cmd", {
      id: props.stickerId,
      patch: { title, content: draft.value },
    });
  } catch (e) {
    console.error("[ui] 保存失败：", e);
    return;
  }
  emit("saved");
}

/** 取消：不保存，退出编辑。 */
function cancel() {
  emit("cancelled");
}

/** Tab 插入两个空格（保持 Markdown 缩进习惯）；Ctrl+S 保存。 */
function onKeydown(e: KeyboardEvent) {
  if (e.key === "Tab") {
    e.preventDefault();
    const el = textarea.value;
    if (!el) return;
    const start = el.selectionStart;
    const end = el.selectionEnd;
    draft.value = draft.value.slice(0, start) + "  " + draft.value.slice(end);
    requestAnimationFrame(() => {
      el.setSelectionRange(start + 2, start + 2);
    });
  } else if ((e.ctrlKey || e.metaKey) && (e.key === "s" || e.key === "S")) {
    e.preventDefault();
    save();
  }
}

onMounted(() => {
  settings.refresh();
  textarea.value?.focus();
});

// 供 StickerWindow overlay 的保存/取消按钮调用
defineExpose({ save, cancel });
</script>

<template>
  <!-- Markdown 原生文本编辑区：透明背景、无聚焦高亮。
       保存/取消按钮在 StickerWindow 的 overlay 上（与交互模式按钮同风格） -->
  <textarea
    ref="textarea"
    v-model="draft"
    class="src"
    :style="{ fontSize: editFontSize + 'px' }"
    spellcheck="false"
    @keydown="onKeydown"
  ></textarea>
</template>

<style scoped>
.src {
  width: 100%;
  height: 100%;
  box-sizing: border-box;
  border: none;
  outline: none;
  resize: none;
  background: transparent;
  color: #333;
  line-height: 1.7;
  padding: 8px 6px;
  font-family: inherit;
}

.src:focus {
  background: transparent;
  box-shadow: none;
}
</style>
