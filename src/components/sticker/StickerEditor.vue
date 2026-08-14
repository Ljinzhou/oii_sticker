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

/** Tab 插入两个空格（保持 Markdown 缩进习惯）。 */
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
  }
}

onMounted(() => {
  settings.refresh();
  textarea.value?.focus();
});
</script>

<template>
  <div class="editor">
    <!-- 左上角：保存（蓝底）/ 取消（白底），样式与交互模式蒙版按钮一致 -->
    <div class="actions">
      <button class="ov-btn save" @click="save">保存</button>
      <button class="ov-btn" @click="cancel">取消</button>
    </div>

    <!-- Markdown 原生文本编辑区：透明背景、无聚焦高亮 -->
    <textarea
      ref="textarea"
      v-model="draft"
      class="src"
      :style="{ fontSize: editFontSize + 'px' }"
      spellcheck="false"
      @keydown="onKeydown"
    ></textarea>
  </div>
</template>

<style scoped>
.editor {
  position: relative;
  display: flex;
  flex-direction: column;
  min-height: 100%;
  height: 100%;
}

/* 左上角操作按钮：与交互模式蒙版按钮（ov-btn）同风格 */
.actions {
  position: absolute;
  top: 0;
  left: 0;
  z-index: 15;
  display: flex;
  gap: 6px;
  padding: 4px;
}

.ov-btn {
  border: none;
  background: rgba(255, 255, 255, 0.85);
  border-radius: 8px;
  padding: 4px 12px;
  font-size: 12px;
  color: #555;
  cursor: pointer;
  box-shadow: 0 1px 4px rgba(0, 0, 0, 0.15);
  transition: background 0.15s;
}

.ov-btn:hover {
  background: #fff;
  color: #4f7cff;
}

.ov-btn.save {
  background: #4f7cff;
  color: #fff;
}

.ov-btn.save:hover {
  background: #3b67e8;
  color: #fff;
}

/* Markdown 原生文本编辑区：透明、无边框、无聚焦高亮 */
.src {
  flex: 1;
  width: 100%;
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
