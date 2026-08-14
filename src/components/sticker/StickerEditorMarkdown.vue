<script setup lang="ts">
// Markdown 编辑模式：原生文本 textarea + 行号 gutter + highlight.js 行级高亮层。
// 高亮层（pre）覆盖在 textarea 下方：文字透明只留光标（caret），滚动同步，
// 宽度按 textarea 实际内容区（clientWidth）精确对齐，保证换行位置一致。
import { ref, computed, watch, onMounted, onBeforeUnmount } from "vue";
import { highlightMarkdown } from "../../utils/markdown-highlight";

const props = defineProps<{
  modelValue: string;
  fontSize: number;
  showLineNumbers: boolean;
}>();

const emit = defineEmits<{ "update:modelValue": [value: string] }>();

const textarea = ref<HTMLTextAreaElement | null>(null);
const gutter = ref<HTMLDivElement | null>(null);
const hlLayer = ref<HTMLPreElement | null>(null);

const text = computed({
  get: () => props.modelValue,
  set: (v: string) => emit("update:modelValue", v),
});

// 行数 = 逻辑行数（按 \n 计）
const lineCount = computed(() => props.modelValue.split("\n").length);

// 高亮层：防抖 120ms 重算（大文本不卡输入）
const hlHtml = ref("");
let hlTimer: number | undefined;
function scheduleHighlight() {
  if (hlTimer) window.clearTimeout(hlTimer);
  hlTimer = window.setTimeout(() => {
    hlHtml.value = highlightMarkdown(props.modelValue);
  }, 120);
}
watch(() => props.modelValue, scheduleHighlight, { immediate: true });

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
  scheduleHighlight();
  alignWidths();
  observer = new ResizeObserver(alignWidths);
  if (textarea.value) observer.observe(textarea.value);
});

onBeforeUnmount(() => {
  if (hlTimer) window.clearTimeout(hlTimer);
  observer?.disconnect();
});

/** Tab 插入两个空格（保持 Markdown 缩进习惯）。 */
function onKeydown(e: KeyboardEvent) {
  if (e.key === "Tab") {
    e.preventDefault();
    const el = textarea.value;
    if (!el) return;
    const start = el.selectionStart;
    const end = el.selectionEnd;
    text.value = props.modelValue.slice(0, start) + "  " + props.modelValue.slice(end);
    requestAnimationFrame(() => {
      el.setSelectionRange(start + 2, start + 2);
    });
  }
}
</script>

<template>
  <div class="editor">
    <!-- 行号区 -->
    <div v-if="showLineNumbers" ref="gutter" class="gutter" :style="{ fontSize: fontSize + 'px' }">
      <div v-for="n in lineCount" :key="n" class="ln">{{ n }}</div>
    </div>

    <!-- 高亮层 + 文本层 -->
    <div class="hl-wrap">
      <pre
        ref="hlLayer"
        class="hl-layer"
        :style="{ fontSize: fontSize + 'px' }"
        aria-hidden="true"
        v-html="hlHtml"
      ></pre>
      <textarea
        ref="textarea"
        :value="props.modelValue"
        class="src"
        :style="{ fontSize: fontSize + 'px' }"
        spellcheck="false"
        @input="text = ($event.target as HTMLTextAreaElement).value"
        @keydown="onKeydown"
        @scroll="syncScroll"
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
  font-family: Consolas, "Courier New", monospace;
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
  font-family: Consolas, "Courier New", monospace;
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
  font-family: Consolas, "Courier New", monospace;
}

.src:focus {
  background: transparent;
  box-shadow: none;
}

/* 选区仍可见（文字透明时） */
.src::selection {
  background: rgba(79, 124, 255, 0.25);
}
</style>
