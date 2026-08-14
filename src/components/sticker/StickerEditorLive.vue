<script setup lang="ts">
// 即时预览编辑模式（Typora/Obsidian 式）：CodeMirror 6 内核。
// Phase A：基础编辑（行号/折行/缩进/语法高亮）+ 防抖回写 + flush（保存前强制回写）。
// 后续阶段（B-E）：行内渲染 decoration、光标穿越、块级交互、斜杠菜单、工具栏等。
import { ref, watch, onMounted, onBeforeUnmount } from "vue";
import type { EditorView } from "@codemirror/view";
import { createLiveView, setLiveDoc, setLiveFontSize } from "./live/LiveEditorView";

const props = defineProps<{
  modelValue: string;
  fontSize: number;
}>();

const emit = defineEmits<{
  "update:modelValue": [value: string];
  save: [];
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
    onDocChange: scheduleEmit,
    onSave: () => emit("save"),
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
</style>
