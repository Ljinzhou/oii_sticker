<script setup lang="ts">
// 及时预览编辑模式（Typora/Obsidian 式）：CodeMirror 6 内核。
// Phase A：基础编辑（行号/折行/缩进/语法高亮）+ 防抖回写 + flush（保存前强制回写）。
// 后续阶段（B-E）：行内渲染 decoration、光标穿越、块级交互、斜杠菜单、工具栏等。
import { ref, watch, onMounted, onBeforeUnmount } from "vue";
import type { EditorView } from "@codemirror/view";
import {
  createLiveView,
  setLiveDoc,
  setLiveFontFamily,
  setLiveFontSize,
  setLiveLineNumbers,
  setLiveTodoBlocksInView,
} from "./live/LiveEditorView";
import type { TodoBlock } from "../../types";
import type { SlashAnchor } from "../slash/types";

const props = defineProps<{
  modelValue: string;
  fontSize: number;
  fontFamily: string;
  showLineNumbers: boolean;
  todoBlocks: TodoBlock[];
  slashOpen: boolean;
}>();

const emit = defineEmits<{
  "update:modelValue": [value: string];
  save: [];
  slash: [query: string, from: number, to: number, anchor: SlashAnchor];
  slashClose: [];
  openTodo: [id: string];
  slashNav: [dir: 1 | -1];
  slashConfirm: [];
  slashCancel: [];
}>();

const host = ref<HTMLDivElement | null>(null);
let view: EditorView | null = null;
let emitTimer: number | undefined;

/** 用户编辑 → 防抖 400ms 回写（避免每次按键触发父级渲染链路）。
 *  内容与当前 props 相同（外部同步回显）时跳过，防止无意义回写。 */
function scheduleEmit(doc: string) {
  if (doc === props.modelValue) return;
  if (emitTimer) window.clearTimeout(emitTimer);
  emitTimer = window.setTimeout(() => {
    emitTimer = undefined;
    emit("update:modelValue", doc);
  }, 400);
}

/** 立即回写（保存前调用，避免防抖窗口内内容丢失）。 */
function flush() {
  if (emitTimer) window.clearTimeout(emitTimer);
  emitTimer = undefined;
  if (view) {
    emit("update:modelValue", view.state.doc.toString());
  }
}

onMounted(() => {
  if (!host.value) return;
  view = createLiveView(host.value, {
    doc: props.modelValue,
    fontSize: props.fontSize,
    fontFamily: props.fontFamily,
    showLineNumbers: props.showLineNumbers,
    onDocChange: scheduleEmit,
    onSave: () => emit("save"),
    onSlash: (query, from, to, anchor) => emit("slash", query, from, to, anchor),
    onSlashClose: () => emit("slashClose"),
    onTodoOpen: (id) => emit("openTodo", id),
    todoBlocks: props.todoBlocks,
    slashOpen: () => props.slashOpen,
    onSlashNav: (dir) => emit("slashNav", dir),
    onSlashConfirm: () => emit("slashConfirm"),
    onSlashCancel: () => emit("slashCancel"),
  });
  view.focus();
});

// 外部内容更新（保存后 load / push-update）→ 同步进编辑器
watch(
  () => props.modelValue,
  (v) => {
    if (view) setLiveDoc(view, v);
  },
);

// 编辑字号实时生效
watch(
  () => props.fontSize,
  (v) => {
    if (view) setLiveFontSize(view, v);
  },
);

watch(
  () => props.fontFamily,
  (v) => {
    if (view) setLiveFontFamily(view, v);
  },
);

// 行号显示实时生效（与 Markdown 编辑模式共用系统设置 editor_line_numbers）
watch(
  () => props.showLineNumbers,
  (v) => {
    if (view) setLiveLineNumbers(view, v);
  },
);

watch(
  () => props.todoBlocks,
  (blocks) => {
    if (view) setLiveTodoBlocksInView(view, blocks);
  },
  { deep: true },
);

onBeforeUnmount(() => {
  if (emitTimer) window.clearTimeout(emitTimer);
  view?.destroy();
  view = null;
});

defineExpose({ flush });
</script>

<template>
  <div ref="host" class="live-host"></div>
</template>

<style scoped>
.live-host {
  width: 100%;
  height: 100%;
  overflow: hidden;
}

.live-host :deep(.cm-editor) {
  height: 100%;
}

.live-host :deep(.cm-editor.cm-focused) {
  outline: none;
}

/* ── 行内渲染 widget 样式（Live Preview） ── */
.live-host :deep(.live-render.live-strong strong) {
  font-weight: 700;
}
.live-host :deep(.live-render.live-em em) {
  font-style: italic;
}
.live-host :deep(.live-render.live-del del) {
  text-decoration: line-through;
  opacity: 0.75;
}
.live-host :deep(.live-render.live-code code) {
  background: rgba(0, 0, 0, 0.06);
  border-radius: 4px;
  padding: 1px 5px;
  font-family: inherit;
  font-size: 0.92em;
}
.live-host :deep(.live-render.live-link a) {
  color: #4f7cff;
  text-decoration: none;
  cursor: pointer;
}
.live-host :deep(.live-render.live-math .math-inline) {
  color: inherit;
}

/* 任务 checkbox（替换 [ ] 标记的 widget） */
.live-host :deep(.live-task-checkbox) {
  margin: 0 4px 0 2px;
  accent-color: #4f7cff;
  cursor: pointer;
  vertical-align: middle;
}

/* ── 块级渲染（标题/列表/引用/分隔线） ── */
.live-host :deep(.cm-live-h1) {
  font-size: 1.5em;
  font-weight: 700;
  line-height: 1.3;
  text-decoration: none !important;
}
.live-host :deep(.cm-live-h2) {
  font-size: 1.3em;
  font-weight: 700;
  line-height: 1.3;
  text-decoration: none !important;
}
.live-host :deep(.cm-live-h3) {
  font-size: 1.15em;
  font-weight: 700;
  text-decoration: none !important;
}
.live-host :deep(.cm-live-h4),
.live-host :deep(.cm-live-h5),
.live-host :deep(.cm-live-h6) {
  font-weight: 700;
  text-decoration: none !important;
}

.live-host :deep(.live-listmark) {
  color: inherit;
  font-weight: inherit;
  display: inline-block;
  min-width: 2.2ch;
  text-align: right;
  margin-right: 4px;
}

.live-host :deep(.cm-live-quote) {
  display: inline-block;
  border-left: 3px solid rgba(0, 0, 0, 0.15);
  padding-left: 8px;
  opacity: 0.85;
}

.live-host :deep(.live-hr) {
  height: 1px;
  background: rgba(0, 0, 0, 0.15);
  margin: 10px 0;
  width: 100%;
}

.live-host :deep(.live-code-block) {
  display: block;
  max-width: 100%;
  overflow-x: auto;
  padding: 4px 0;
}
.live-host :deep(.live-code-block pre) {
  margin: 0;
  min-width: 0;
  white-space: pre;
}
.live-host :deep(.live-code-block code) {
  display: block;
  min-width: max-content;
  padding: 8px 10px;
  border-radius: 4px;
  background: rgba(0, 0, 0, 0.06);
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 0.92em;
  line-height: 1.5;
}

.live-host :deep(.live-math-block .math-block) {
  margin: 8px 0;
  overflow-x: auto;
  color: inherit;
}

.live-host :deep(.live-todo-block),
.live-host :deep(.live-done-block) {
  margin: 8px 0;
}
.live-host :deep(.todo-block-card),
.live-host :deep(.done-block-card) {
  overflow: hidden;
  border: 1px solid rgba(0, 0, 0, 0.1);
  border-radius: 6px;
  background: rgba(255, 255, 255, 0.42);
}
.live-host :deep(.tb-head) {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 8px 10px;
  border-bottom: 1px solid rgba(0, 0, 0, 0.06);
}
.live-host :deep(.tb-title) {
  flex: 1 1 0%;
  min-width: 0;
  overflow: hidden;
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.live-host :deep(.tb-count),
.live-host :deep(.done-block-card summary) {
  color: #777;
  font-size: 12px;
}
.live-host :deep(.tb-list),
.live-host :deep(.db-list) {
  margin: 0;
  padding: 6px 10px;
  list-style: none;
}
.live-host :deep(.tb-list li),
.live-host :deep(.db-list li) {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 24px;
}
.live-host :deep(.tb-sub) { padding-left: 22px; }
.live-host :deep(.tb-done) { color: #999; text-decoration: line-through; }
.live-host :deep(.done-block-card) { padding: 7px 10px; }
.live-host :deep(.todo-task-checkbox) { accent-color: #4f7cff; }

/* 复合编号行缩进（按嵌套深度，模拟 Obsidian 层级） */
.live-host :deep(.cm-live-n1) {
  padding-left: 1.3em;
}
.live-host :deep(.cm-live-n2) {
  padding-left: 2.6em;
}
.live-host :deep(.cm-live-n3) {
  padding-left: 3.9em;
}
.live-host :deep(.cm-live-n4) {
  padding-left: 5.2em;
}
</style>
