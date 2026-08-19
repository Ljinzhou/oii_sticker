<script setup lang="ts">
// Markdown 编辑模式：原生文本 textarea + 行号 gutter + highlight.js 行级高亮层。
// 高亮层（pre）覆盖在 textarea 下方：文字透明只留光标（caret），滚动同步，
// 宽度按 textarea 实际内容区（clientWidth）精确对齐，保证换行位置一致。
import { ref, computed, onMounted, onBeforeUnmount } from "vue";
import { highlightMarkdown } from "../../utils/markdown-highlight";
import {
  handleEnterAtCursor,
  handleTabAtCursor,
  handleShiftTabAtCursor,
} from "../../utils/edit-actions";

const props = defineProps<{
  modelValue: string;
  fontSize: number;
  fontFamily: string;
  showLineNumbers: boolean;
}>();

const emit = defineEmits<{ "update:modelValue": [value: string]; slash: [query: string, from: number, to: number]; slashClose: []; openTodo: [id: string] }>();

const textarea = ref<HTMLTextAreaElement | null>(null);
const gutter = ref<HTMLDivElement | null>(null);
const hlLayer = ref<HTMLPreElement | null>(null);

const text = computed({
  get: () => props.modelValue,
  set: (v: string) => emit("update:modelValue", v),
});

// 行数 = 逻辑行数（按 \n 计）
const lineCount = computed(() => props.modelValue.split("\n").length);

// 高亮层：同步 computed（无防抖）——textarea 文字透明，高亮层必须
// 与输入同步刷新，否则输入字符延迟可见（表现为卡顿）。
// 整块 hljs 高亮后普通文本单次 <5ms，便签场景可接受。
const hlHtml = computed(() => highlightMarkdown(props.modelValue));

/** textarea 滚动 → 同步高亮层与行号区。 */
function syncScroll() {
  if (hlLayer.value && textarea.value) {
    hlLayer.value.scrollTop = textarea.value.scrollTop;
  }
  if (gutter.value && textarea.value) {
    gutter.value.scrollTop = textarea.value.scrollTop;
  }
}

/** 高亮层右缘对齐 textarea 内容区（扣除滚动条宽度），保证换行位置一致。 */
function alignWidths() {
  const ta = textarea.value;
  const pre = hlLayer.value;
  if (ta && pre) {
    pre.style.right = `${ta.offsetWidth - ta.clientWidth}px`;
  }
}

let observer: ResizeObserver | undefined;

onMounted(() => {
  alignWidths();
  observer = new ResizeObserver(alignWidths);
  if (textarea.value) observer.observe(textarea.value);
});

onBeforeUnmount(() => {
  observer?.disconnect();
});

/** Enter / Tab / Shift+Tab 智能编辑（列表续行、嵌套缩进；围栏内退化）。
 *  逻辑移植自 Rust editing 模块（handle_enter/tab/shift_tab_at_cursor）。 */
function onKeydown(e: KeyboardEvent) {
  const el = textarea.value;
  if (!el) return;
  const start = el.selectionStart;
  const end = el.selectionEnd;

  if (e.key === "Tab") {
    e.preventDefault();
    if (start !== end) {
      // 多行/多字符选区：整体缩进（每行前加 2 空格）——简化为默认 2 空格缩进
      const prefix = props.modelValue.slice(0, start);
      const sel = props.modelValue.slice(start, end);
      const suffix = props.modelValue.slice(end);
      const indented = sel
        .split("\n")
        .map((l, i) => (i === 0 && prefix.endsWith("\n") ? `  ${l}` : i > 0 ? `  ${l}` : l))
        .join("\n");
      const added = indented.length - sel.length;
      text.value = `${prefix}${indented}${suffix}`;
      requestAnimationFrame(() => {
        el.setSelectionRange(start + (prefix.endsWith("\n") ? 2 : 0), end + added);
      });
      return;
    }
    if (e.shiftKey) {
      const r = handleShiftTabAtCursor(props.modelValue, start);
      if (r) {
        text.value = r.text;
        requestAnimationFrame(() => el.setSelectionRange(r.cursor, r.cursor));
      }
    } else {
      const r = handleTabAtCursor(props.modelValue, start);
      text.value = r.text;
      requestAnimationFrame(() => el.setSelectionRange(r.cursor, r.cursor));
    }
  } else if (e.key === "Enter" && start === end) {
    // 列表行智能续行（非列表行交给默认回车）
    const r = handleEnterAtCursor(props.modelValue, start);
    if (r) {
      e.preventDefault();
      text.value = r.text;
      requestAnimationFrame(() => el.setSelectionRange(r.cursor, r.cursor));
    }
  }
}

function onInput(event: Event) {
  const el = event.target as HTMLTextAreaElement;
  text.value = el.value;
  const before = el.value.slice(0, el.selectionStart);
  const match = /(?:^|\n)\/([^\s/]*)$/.exec(before);
  if (match) emit("slash", match[1], el.selectionStart - match[0].length + (match[0].startsWith("\n") ? 1 : 0), el.selectionStart);
  else emit("slashClose");
}

function onClick() {
  const el = textarea.value;
  if (!el) return;
  const before = props.modelValue.slice(0, el.selectionStart);
  const start = before.lastIndexOf("<todo-block");
  const end = props.modelValue.indexOf("</todo-block>", start);
  if (start >= 0 && end >= el.selectionStart) {
    const tag = props.modelValue.slice(start, end + "</todo-block>".length);
    const id = /\bid=["']([^"']+)["']/.exec(tag)?.[1];
    if (id) emit("openTodo", id);
  }
}
</script>

<template>
  <div class="editor">
    <!-- 行号区 -->
    <div v-if="showLineNumbers" ref="gutter" class="gutter" :style="{ fontSize: fontSize + 'px', fontFamily }">
      <div v-for="n in lineCount" :key="n" class="ln">{{ n }}</div>
    </div>

    <!-- 高亮层 + 文本层 -->
    <div class="hl-wrap">
      <pre
        ref="hlLayer"
        class="hl-layer"
        :style="{ fontSize: fontSize + 'px', fontFamily }"
        aria-hidden="true"
        v-html="hlHtml"
      ></pre>
      <textarea
        ref="textarea"
        :value="props.modelValue"
        class="src"
        :style="{ fontSize: fontSize + 'px', fontFamily }"
        spellcheck="false"
        @input="onInput"
        @keydown="onKeydown"
        @scroll="syncScroll"
        @click="onClick"
      ></textarea>
    </div>
  </div>
</template>

<style scoped>
.editor {
  display: flex;
  width: 100%;
  height: 100%;
  overflow: hidden;
}

/* 行号区：与文本层同字号/行高/padding-top 对齐，滚动同步 */
.gutter {
  flex: none;
  overflow: hidden;
  padding: 8px 6px 8px 0;
  border-right: 1px solid rgba(0, 0, 0, 0.08);
  user-select: none;
  line-height: 1.7;
  text-align: right;
  color: rgba(0, 0, 0, 0.3);
  font-family: inherit;
}

.ln {
  padding-right: 8px;
}

/* 高亮层 + 文本层同尺寸/字体/行高/padding：换行位置完全一致 */
.hl-wrap {
  position: relative;
  flex: 1;
  overflow: hidden;
}

.hl-layer {
  position: absolute;
  top: 0;
  bottom: 0;
  left: 0;
  margin: 0;
  overflow: hidden;
  pointer-events: none;
  box-sizing: border-box;
  padding: 8px 6px;
  line-height: 1.7;
  white-space: pre-wrap;
  word-break: break-word;
  overflow-wrap: anywhere;
  font-family: inherit;
  color: #333;
}

.src {
  position: absolute;
  inset: 0;
  box-sizing: border-box;
  width: 100%;
  height: 100%;
  border: none;
  outline: none;
  resize: none;
  background: transparent;
  color: transparent; /* 高亮层显示文字，仅保留光标与选区 */
  caret-color: #333;
  line-height: 1.7;
  padding: 8px 6px;
  white-space: pre-wrap;
  word-break: break-word;
  overflow-wrap: anywhere;
  font-family: inherit;
}

.src:focus {
  background: transparent;
  box-shadow: none;
}

/* 选区仍可见（文字透明时） */
.src::selection {
  background: rgba(79, 124, 255, 0.25);
}

/* ── markdown 语法标记着色（主流编辑器风格：标记符号着色、内容保持默认） ── */
.hl-layer :deep(.md-head) {
  color: #a855f7;
  font-weight: 600;
}

.hl-layer :deep(.md-task) {
  color: #16a34a;
  font-weight: 600;
}

.hl-layer :deep(.md-list) {
  color: #4f7cff;
  font-weight: 600;
}

.hl-layer :deep(.md-quote) {
  color: #9ca3af;
}

.hl-layer :deep(.md-hr) {
  color: #9ca3af;
}

.hl-layer :deep(.md-code) {
  color: #d97706;
  background: rgba(0, 0, 0, 0.06);
  border-radius: 3px;
  padding: 0 2px;
  font-family: inherit;
}

.hl-layer :deep(.md-strong) {
  color: #b45309;
  font-weight: 700;
}

.hl-layer :deep(.md-em) {
  color: #b45309;
  font-style: italic;
}

.hl-layer :deep(.md-link) {
  color: #2563eb;
}

.hl-layer :deep(.md-math) {
  color: #7c3aed;
}

.hl-layer :deep(.md-fn) { color: #7c3aed; font-weight: 600; }
</style>
