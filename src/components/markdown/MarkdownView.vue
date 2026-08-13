<script setup lang="ts">
import { computed } from "vue";
import { renderMarkdown } from "../../utils/markdown";

const props = defineProps<{
  content: string;
  interactive: boolean;
}>();

const emit = defineEmits<{ toggle: [line: number] }>();

const html = computed(() => renderMarkdown(props.content));

function onContainerClick(e: MouseEvent) {
  const target = e.target as HTMLElement;
  if (target.classList.contains("task-checkbox") && props.interactive) {
    const line = Number(target.dataset.line ?? "-1");
    if (line >= 0) {
      // 先恢复勾选状态（后端会推送最新内容覆盖）
      emit("toggle", line);
    }
  }
}
</script>

<template>
  <div class="markdown" :class="{ interactive }" v-html="html" @click="onContainerClick"></div>
</template>

<style scoped>
.markdown {
  font-size: inherit;
  line-height: 1.7;
  color: inherit;
  word-break: break-word;
  overflow-wrap: anywhere;
}

.markdown :deep(h1),
.markdown :deep(h2),
.markdown :deep(h3),
.markdown :deep(h4) {
  margin: 14px 0 8px;
  line-height: 1.3;
}

.markdown :deep(h1) {
  font-size: 1.5em;
}

.markdown :deep(h2) {
  font-size: 1.3em;
}

.markdown :deep(h3) {
  font-size: 1.15em;
}

.markdown :deep(p) {
  margin: 6px 0;
}

.markdown :deep(ul),
.markdown :deep(ol) {
  margin: 6px 0;
  padding-left: 22px;
}

.markdown :deep(li) {
  margin: 2px 0;
}

.markdown :deep(blockquote) {
  margin: 8px 0;
  padding: 4px 12px;
  border-left: 3px solid rgba(0, 0, 0, 0.15);
  color: inherit;
  opacity: 0.85;
}

.markdown :deep(code) {
  background: rgba(0, 0, 0, 0.06);
  border-radius: 4px;
  padding: 1px 5px;
  font-family: Consolas, "Courier New", monospace;
  font-size: 0.92em;
}

.markdown :deep(pre) {
  background: rgba(0, 0, 0, 0.06);
  border-radius: 8px;
  padding: 10px 12px;
  overflow-x: auto;
}

.markdown :deep(pre code) {
  background: none;
  padding: 0;
}

.markdown :deep(a) {
  color: #4f7cff;
}

.markdown :deep(hr) {
  border: none;
  border-top: 1px solid rgba(0, 0, 0, 0.12);
  margin: 12px 0;
}

.markdown :deep(table) {
  border-collapse: collapse;
  margin: 8px 0;
}

.markdown :deep(th),
.markdown :deep(td) {
  border: 1px solid rgba(0, 0, 0, 0.15);
  padding: 4px 10px;
  font-size: 0.95em;
}

.markdown :deep(img) {
  max-width: 100%;
  border-radius: 6px;
}

/* 任务清单 checkbox */
.markdown :deep(.task-item) {
  list-style: none;
  margin-left: -22px;
}

.markdown :deep(.task-checkbox) {
  margin-right: 8px;
  accent-color: #4f7cff;
  cursor: pointer;
  vertical-align: middle;
}

.markdown :deep(.task-checkbox:disabled) {
  cursor: default;
}
</style>
