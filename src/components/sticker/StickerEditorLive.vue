<script setup lang="ts">
// 即时预览编辑模式（Typora 式）：contenteditable 渲染视图直接编辑。
// 输入 → 防抖回写 Markdown（htmlToMarkdown）→ 重渲染 → 恢复光标（文本偏移定位）。
// 公式为整体对象（contenteditable=false + data-tex），保存时保真回写 $..$。
import { ref, computed, watch, nextTick, onMounted, onBeforeUnmount } from "vue";
import { renderMarkdownEditable, htmlToMarkdown } from "../../utils/markdown-editable";

const props = defineProps<{
  modelValue: string;
  fontSize: number;
}>();

const emit = defineEmits<{ "update:modelValue": [value: string] }>();

const root = ref<HTMLDivElement | null>(null);

const html = computed(() => renderMarkdownEditable(props.modelValue));

// ── 光标保持（文本偏移 → 节点定位） ──
type CursorPos = { start: number; end: number } | null;
let savedCursor: CursorPos = null;

function saveCursor(): CursorPos {
  const el = root.value;
  const sel = window.getSelection();
  if (!el || !sel || sel.rangeCount === 0 || !el.contains(sel.anchorNode)) return null;
  const range = sel.getRangeAt(0);
  const pre = document.createRange();
  pre.selectNodeContents(el);
  pre.setEnd(range.startContainer, range.startOffset);
  const start = pre.toString().length;
  return { start, end: start + range.toString().length };
}

function restoreCursor(pos: CursorPos) {
  const el = root.value;
  if (!pos || !el) return;
  const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
  let acc = 0;
  let startNode: Node | null = null;
  let startOff = 0;
  let endNode: Node | null = null;
  let endOff = 0;
  let node: Node | null;
  while ((node = walker.nextNode())) {
    const len = node.textContent?.length ?? 0;
    if (!startNode && acc + len >= pos.start) {
      startNode = node;
      startOff = pos.start - acc;
    }
    if (!endNode && acc + len >= pos.end) {
      endNode = node;
      endOff = pos.end - acc;
    }
    acc += len;
    if (startNode && endNode) break;
  }
  const sn = startNode;
  if (!sn) return;
  const range = document.createRange();
  range.setStart(sn, startOff);
  range.setEnd(endNode ?? sn, endNode ? endOff : startOff);
  const sel = window.getSelection();
  sel?.removeAllRanges();
  sel?.addRange(range);
}

// ── 回写（防抖）与渲染后光标恢复 ──
let emitTimer: number | undefined;

function scheduleEmit() {
  if (emitTimer) window.clearTimeout(emitTimer);
  emitTimer = window.setTimeout(() => {
    emitTimer = undefined;
    emit("update:modelValue", htmlToMarkdown(root.value?.innerHTML ?? ""));
  }, 400);
}

/** 立即回写（保存前调用，避免防抖窗口内内容丢失）。 */
function flush() {
  if (emitTimer) window.clearTimeout(emitTimer);
  emitTimer = undefined;
  emit("update:modelValue", htmlToMarkdown(root.value?.innerHTML ?? ""));
}

// 模型内容变化（含自己回写）→ 重渲染 → 恢复光标
watch(html, async () => {
  await nextTick();
  restoreCursor(savedCursor);
  savedCursor = null;
});

function onInput(e: InputEvent) {
  if (e.isComposing) return; // 输入法组词期间不打断
  savedCursor = saveCursor();
  scheduleEmit();
}

function onKeydown(e: KeyboardEvent) {
  // 公式整体对象上按删除/退格 = 删除整块（contenteditable=false 默认已如此）
  if (e.key === "Tab") {
    e.preventDefault();
    document.execCommand("insertText", false, "  ");
  }
}

function onPaste(e: ClipboardEvent) {
  e.preventDefault();
  const textData = e.clipboardData?.getData("text/plain") ?? "";
  document.execCommand("insertText", false, textData);
}

onMounted(() => {
  root.value?.focus();
});

onBeforeUnmount(() => {
  if (emitTimer) window.clearTimeout(emitTimer);
});

defineExpose({ flush });
</script>

<template>
  <div
    ref="root"
    class="live"
    :style="{ fontSize: fontSize + 'px' }"
    contenteditable="true"
    spellcheck="false"
    v-html="html"
    @input="onInput"
    @keydown="onKeydown"
    @paste="onPaste"
  ></div>
</template>

<style scoped>
.live {
  width: 100%;
  height: 100%;
  box-sizing: border-box;
  outline: none;
  overflow-y: auto;
  line-height: 1.7;
  color: #333;
  padding: 8px 6px;
  word-break: break-word;
  overflow-wrap: anywhere;
  caret-color: #333;
}

/* 复用渲染视图的 markdown 排版样式（标题/列表/引用/代码/表格等） */
.live :deep(h1),
.live :deep(h2),
.live :deep(h3),
.live :deep(h4) {
  margin: 14px 0 8px;
  line-height: 1.3;
}
.live :deep(h1) {
  font-size: 1.5em;
}
.live :deep(h2) {
  font-size: 1.3em;
}
.live :deep(h3) {
  font-size: 1.15em;
}
.live :deep(p) {
  margin: 6px 0;
}
.live :deep(ul),
.live :deep(ol) {
  margin: 6px 0;
  padding-left: 22px;
}
.live :deep(li) {
  margin: 2px 0;
}
.live :deep(blockquote) {
  margin: 8px 0;
  padding: 4px 12px;
  border-left: 3px solid rgba(0, 0, 0, 0.15);
  color: inherit;
  opacity: 0.85;
}
.live :deep(code) {
  background: rgba(0, 0, 0, 0.06);
  border-radius: 4px;
  padding: 1px 5px;
  font-family: Consolas, "Courier New", monospace;
  font-size: 0.92em;
}
.live :deep(pre) {
  background: rgba(0, 0, 0, 0.06);
  border-radius: 8px;
  padding: 10px 12px;
  overflow-x: auto;
}
.live :deep(pre code) {
  background: none;
  padding: 0;
}
.live :deep(a) {
  color: #4f7cff;
}
.live :deep(hr) {
  border: none;
  border-top: 1px solid rgba(0, 0, 0, 0.12);
  margin: 12px 0;
}
.live :deep(table) {
  border-collapse: collapse;
  margin: 8px 0;
}
.live :deep(th),
.live :deep(td) {
  border: 1px solid rgba(0, 0, 0, 0.15);
  padding: 4px 10px;
  font-size: 0.95em;
}
.live :deep(img) {
  max-width: 100%;
  border-radius: 6px;
}

/* 数学公式整体对象：不可编辑内部，hover 提示 */
.live :deep(.math-inline),
.live :deep(.math-block) {
  cursor: default;
  outline: 1px dashed transparent;
  border-radius: 4px;
  transition: outline-color 0.15s;
}
.live :deep(.math-inline:hover),
.live :deep(.math-block:hover) {
  outline-color: rgba(79, 124, 255, 0.5);
}
.live :deep(.math-block) {
  margin: 8px 0;
  overflow-x: auto;
}
</style>
