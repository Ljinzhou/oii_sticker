<script setup lang="ts">
import { ref, computed, onMounted } from "vue";
import { invoke } from "../../composables/useTauri";
import { useSettingsStore } from "../../stores/settings";
import { renderMarkdownEditable, htmlToMarkdown } from "../../utils/markdown";

const props = defineProps<{
  content: string;
  stickerId: number;
}>();

const emit = defineEmits<{
  saved: [];
  cancelled: [];
  closed: [];
}>();

const settings = useSettingsStore();
const editableEl = ref<HTMLElement | null>(null);

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

/** 保存：把编辑后的 HTML 转回 Markdown 并落库。 */
async function save() {
  const md = htmlToMarkdown(editableEl.value?.innerHTML ?? "");
  const title = extractTitle(md);
  try {
    await invoke("update_sticker_cmd", {
      id: props.stickerId,
      patch: { title, content: md },
    });
  } catch (e) {
    console.error("[ui] 保存失败：", e);
    return;
  }
  emit("saved");
}

/** 取消：恢复原始内容并退出编辑。 */
function cancel() {
  if (editableEl.value) {
    editableEl.value.innerHTML = renderMarkdownEditable(props.content);
  }
  emit("cancelled");
}

/** 编辑模式下 Tab 键插入两个空格（保持缩进习惯）。 */
function onKeydown(e: KeyboardEvent) {
  if (e.key === "Tab") {
    e.preventDefault();
    document.execCommand("insertText", false, "  ");
  }
}

onMounted(() => {
  settings.refresh();
  if (editableEl.value) {
    editableEl.value.innerHTML = renderMarkdownEditable(props.content);
  }
});
</script>

<template>
  <div class="editor">
    <!-- 左上角：保存 / 取消 / 关闭 -->
    <div class="bar">
      <button class="btn primary" @click="save">保存</button>
      <button class="btn" @click="cancel">取消</button>
      <button class="btn close" title="关闭窗口" @click="emit('closed')">✕</button>
      <span class="tip">点击内容直接编辑，保存后自动转回 Markdown</span>
    </div>

    <!-- WYSIWYG：Markdown 渲染视图上直接编辑 -->
    <div
      ref="editableEl"
      class="editable"
      contenteditable="true"
      :style="{ fontSize: editFontSize + 'px' }"
      spellcheck="false"
      @keydown="onKeydown"
    ></div>
  </div>
</template>

<style scoped>
.editor {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-height: 100%;
}

/* 左上角操作条（悬浮，不占内容布局） */
.bar {
  position: sticky;
  top: 0;
  z-index: 12;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  border-radius: 10px;
  background: rgba(255, 255, 255, 0.92);
  box-shadow: 0 2px 10px rgba(0, 0, 0, 0.1);
  backdrop-filter: blur(6px);
}

.tip {
  font-size: 11px;
  color: #999;
  margin-left: 4px;
}

.btn {
  border: 1px solid rgba(0, 0, 0, 0.12);
  border-radius: 8px;
  padding: 5px 14px;
  font-size: 13px;
  background: #fff;
  color: #333;
  cursor: pointer;
}

.btn:hover {
  background: #f2f4f7;
}

.btn.primary {
  background: #4f7cff;
  border-color: #4f7cff;
  color: #fff;
}

.btn.close:hover {
  background: #ffe3e3;
  color: #d33;
}

/* WYSIWYG 编辑区：与交互模式渲染样式一致（复用 markdown 视觉） */
.editable {
  flex: 1;
  overflow-y: auto;
  outline: none;
  line-height: 1.7;
  color: #333;
  word-break: break-word;
  overflow-wrap: anywhere;
  border-radius: 8px;
  padding: 4px 6px;
}

.editable:focus {
  background: rgba(255, 255, 255, 0.28);
}

.editable :deep(h1),
.editable :deep(h2),
.editable :deep(h3),
.editable :deep(h4) {
  margin: 14px 0 8px;
  line-height: 1.3;
}

.editable :deep(h1) {
  font-size: 1.5em;
}

.editable :deep(h2) {
  font-size: 1.3em;
}

.editable :deep(h3) {
  font-size: 1.15em;
}

.editable :deep(p) {
  margin: 6px 0;
}

.editable :deep(ul),
.editable :deep(ol) {
  margin: 6px 0;
  padding-left: 22px;
}

.editable :deep(li) {
  margin: 2px 0;
}

.editable :deep(blockquote) {
  margin: 8px 0;
  padding: 4px 12px;
  border-left: 3px solid rgba(0, 0, 0, 0.15);
  opacity: 0.85;
}

.editable :deep(code) {
  background: rgba(0, 0, 0, 0.06);
  border-radius: 4px;
  padding: 1px 5px;
  font-family: Consolas, "Courier New", monospace;
  font-size: 0.92em;
}

.editable :deep(pre) {
  background: rgba(0, 0, 0, 0.06);
  border-radius: 8px;
  padding: 10px 12px;
  overflow-x: auto;
}

.editable :deep(a) {
  color: #4f7cff;
}

.editable :deep(hr) {
  border: none;
  border-top: 1px solid rgba(0, 0, 0, 0.12);
  margin: 12px 0;
}

.editable :deep(table) {
  border-collapse: collapse;
  margin: 8px 0;
}

.editable :deep(th),
.editable :deep(td) {
  border: 1px solid rgba(0, 0, 0, 0.15);
  padding: 4px 10px;
}
</style>
