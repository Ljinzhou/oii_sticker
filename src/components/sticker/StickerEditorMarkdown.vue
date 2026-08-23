<script setup lang="ts">
// Markdown 编辑模式：CodeMirror 6 源码内核（原生文本编辑，不渲染 inline decoration）。
// 行号显示直接复用及时预览的行号代码（makeShowLineNumbers / lightTheme），
// 智能编辑（Enter/Tab/Shift+Tab）复用 liveTransforms（与及时预览同一份变换），
// 斜杠菜单复用 reportSlash 与光标锚点计算；不重写行号显示逻辑，功能逻辑保持一致。
import { ref, watch, onMounted, onBeforeUnmount } from "vue";
import type { EditorView } from "@codemirror/view";
import {
  createMarkdownSourceView,
  setMarkdownSourceDoc,
  setMarkdownSourceFontFamily,
  setMarkdownSourceFontSize,
  setMarkdownSourceLineNumbers,
} from "./live/sourceEditor";
import type { SlashAnchor } from "../slash/types";

const props = defineProps<{
  modelValue: string;
  fontSize: number;
  fontFamily: string;
  showLineNumbers: boolean;
  slashOpen: boolean;
}>();

const emit = defineEmits<{
  "update:modelValue": [value: string];
  slash: [query: string, from: number, to: number, anchor: SlashAnchor];
  slashClose: [];
  openTodo: [id: string];
  slashNav: [dir: 1 | -1];
  slashConfirm: [];
  slashCancel: [];
}>();

const host = ref<HTMLDivElement | null>(null);
let view: EditorView | null = null;

onMounted(() => {
  if (!host.value) return;
  view = createMarkdownSourceView(host.value, {
    doc: props.modelValue,
    fontSize: props.fontSize,
    fontFamily: props.fontFamily,
    showLineNumbers: props.showLineNumbers,
    onDocChange: (doc) => {
      if (doc !== props.modelValue) emit("update:modelValue", doc);
    },
    onSlash: (query, from, to, anchor) => emit("slash", query, from, to, anchor),
    onSlashClose: () => emit("slashClose"),
    onTodoOpen: (id) => emit("openTodo", id),
    slashOpen: () => props.slashOpen,
    onSlashNav: (dir) => emit("slashNav", dir),
    onSlashConfirm: () => emit("slashConfirm"),
    onSlashCancel: () => emit("slashCancel"),
  });
});

// 外部内容更新（保存后 load / push-update / 斜杠插入）→ 同步进编辑器
watch(
  () => props.modelValue,
  (v) => {
    if (view) setMarkdownSourceDoc(view, v);
  },
);

watch(
  () => props.fontSize,
  (v) => {
    if (view) setMarkdownSourceFontSize(view, v);
  },
);

watch(
  () => props.fontFamily,
  (v) => {
    if (view) setMarkdownSourceFontFamily(view, v);
  },
);

watch(
  () => props.showLineNumbers,
  (v) => {
    if (view) setMarkdownSourceLineNumbers(view, v);
  },
);

onBeforeUnmount(() => {
  view?.destroy();
  view = null;
});
</script>

<template>
  <div ref="host" class="src-host"></div>
</template>

<style scoped>
.src-host {
  width: 100%;
  height: 100%;
  overflow: hidden;
}

.src-host :deep(.cm-editor) {
  height: 100%;
}

.src-host :deep(.cm-editor.cm-focused) {
  outline: none;
}

/* ── 选区仍可见（透明背景主题下保持亮色） ── */
.src-host :deep(.cm-content) {
  background: transparent;
}
</style>
