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
  setLiveUiStateInView,
  flushTableEditing,
} from "./live/LiveEditorView";
import type { TodoBlock } from "../../types";
import type { SlashAnchor } from "../slash/types";
import { useSettingsStore } from "../../stores/settings";
import {
  BLOCK_UI_KEY,
  applyBlockUiAction,
  readBlockUi,
} from "../../utils/block-ui";

const props = defineProps<{
  modelValue: string;
  fontSize: number;
  fontFamily: string;
  showLineNumbers: boolean;
  todoBlocks: TodoBlock[];
  slashOpen: boolean;
  uiKey?: string;
}>();

// 单测等非 Pinia 上下文安全降级：拿不到 store 就不联动（渲染用默认展开态）。
function settingsOrNull() {
  try {
    return useSettingsStore();
  } catch {
    return null;
  }
}

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
/** 最近一次本组件主动回写的内容：识别「自己回写被父级原样传回」的 echo。 */
let lastEmitted: string | null = null;

/** 用户编辑 → 防抖 400ms 回写（避免每次按键触发父级渲染链路）。
 *  关键：进入函数先【无条件】清掉未触发的旧定时器。旧实现先比较后清理，
 *  当"输入又删回与 modelValue 相同的旧值"时会提前 return，留下一个带着
 *  陈旧内容的定时器——它稍后触发就会把刚删掉的字符重新写回编辑器
 *  （表现为退格删不干净、字符复活），并伴随光标被甩到文档开头。 */
function scheduleEmit(doc: string) {
  if (emitTimer) window.clearTimeout(emitTimer);
  emitTimer = undefined;
  // 内容与当前 props 相同（外部同步回显/删回原值）：无需回写
  if (doc === props.modelValue) return;
  emitTimer = window.setTimeout(() => {
    emitTimer = undefined;
    lastEmitted = doc;
    emit("update:modelValue", doc);
  }, 400);
}

/** 立即回写（保存前调用，避免防抖窗口内内容丢失）。 */
function flush() {
  if (emitTimer) window.clearTimeout(emitTimer);
  emitTimer = undefined;
  // 单元格内容在离开单元格时才落盘：保存前必须先提交
  flushTableEditing();
  if (view) {
    const doc = view.state.doc.toString();
    lastEmitted = doc;
    emit("update:modelValue", doc);
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
    ui: readBlockUi(),
    uiKey: props.uiKey || "",
    onBlockUiAction: (action, id) => {
      void applyBlockUiAction(action, id);
      // 持久化写入 settings 后由下方 watcher 统一刷新视图（单一数据流）
    },
    onDoneSource: (sourceId, key) => {
      void applyBlockUiAction("doneSource", key || props.uiKey || "", sourceId);
    },
    slashOpen: () => props.slashOpen,
    onSlashNav: (dir) => emit("slashNav", dir),
    onSlashConfirm: () => emit("slashConfirm"),
    onSlashCancel: () => emit("slashCancel"),
  });
  view.focus();
});

// 外部内容更新（保存后 load / push-update / 斜杠插入）→ 同步进编辑器。
// 注意：本组件自己回写的内容被父级 v-model 原样传回来（echo）时必须跳过——
// 防抖窗口内用户往往又输入了新内容，把 echo 反向同步会拿陈旧值覆盖新输入
// （已删字符复活 / 光标瞬移）。真正的外部更新不受影响，照常同步。
watch(
  () => props.modelValue,
  (v) => {
    if (!view) return;
    if (lastEmitted !== null && v === lastEmitted) {
      lastEmitted = null;
      return;
    }
    setLiveDoc(view, v);
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

// 用户折叠状态（本窗口操作或其它窗口同步）→ 热刷新卡片渲染。
// 只在状态真正变化时 dispatch；程序绝不主动改这份状态，只响应它。
watch(
  () => {
    const s = settingsOrNull();
    return s ? s.config.entries[BLOCK_UI_KEY] : undefined;
  },
  () => {
    if (!view || props.uiKey === undefined) return;
    setLiveUiStateInView(view, readBlockUi(), props.uiKey);
  },
);

onBeforeUnmount(() => {
  if (emitTimer) window.clearTimeout(emitTimer);
  view?.destroy();
  view = null;
});

/** 是否有在途未回写的编辑（外部内容同步时据此避免覆盖用户的即时输入）。 */
function isDirty() {
  if (!view) return false;
  return view.state.doc.toString() !== props.modelValue;
}

defineExpose({ flush, isDirty });
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
  /* 注意：CodeMirror 的 block widget 测量高度时不计外 margin，
     在 widget 根元素上用 margin 会把块下方行号/光标顶位挤乱。
     竖直留白改用 padding（随 widget 高度一起被测量），保证行号与正文对齐。 */
  margin: 0;
  padding: 8px 0;
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
.live-host :deep(.db-head) {
  color: #777;
  font-size: 12px;
}
.live-host :deep(.db-head) {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 7px 10px;
}
.live-host :deep(.db-title) {
  font-weight: 600;
  white-space: nowrap;
}
.live-host :deep(.db-time) {
  color: #aaa;
  font-size: 11px;
  white-space: nowrap;
  margin-left: auto;
}
.live-host :deep(.sd-source) {
  margin-left: auto;
  max-width: 45%;
  font-size: 12px;
  color: #555;
  border: none;
  border-radius: 6px;
  background: rgba(0, 0, 0, 0.05);
  padding: 2px 6px;
  outline: none;
}
.live-host :deep(.db-head .sd-source) {
  margin-left: 8px;
}
.live-host :deep(.tb-caret) {
  flex: none;
  width: 18px;
  text-align: center;
  color: #999;
  font-size: 11px;
  user-select: none;
}
/* 无子任务的行用同宽占位，保持与带折叠按钮的行缩进对齐 */
.live-host :deep(.tb-caret-placeholder) {
  visibility: hidden;
}
/* 空块提示：块内一条任务都没有 */
.live-host :deep(.tb-empty) {
  color: #a9a9a9;
  font-size: 12px;
  min-height: 22px;
  justify-content: center;
  padding: 2px 0;
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
.live-host :deep(.done-block-card) { cursor: pointer; }
.live-host :deep(.todo-task-checkbox) { accent-color: #4f7cff; }

/* ── 渲染态表格（与展示模式 MarkdownView 同一视觉） ── */
.live-host :deep(.live-table-block) {
  padding: 8px 0;
}
.live-host :deep(.live-table-block table) {
  border-collapse: collapse;
  margin: 0;
}
.live-host :deep(.live-table-block th),
.live-host :deep(.live-table-block td) {
  border: 1px solid rgba(0, 0, 0, 0.15);
  padding: 4px 10px;
  font-size: 0.95em;
  line-height: 1.5;
}
.live-host :deep(.live-table-block th) {
  background: rgba(0, 0, 0, 0.03);
  font-weight: 600;
}

/* 表格单元格：渲染态直接编辑 */
.live-host :deep(.live-table-block td),
.live-host :deep(.live-table-block th) {
  position: relative;
  cursor: text;
  outline: none;
}
.live-host :deep(.live-table-block td:focus),
.live-host :deep(.live-table-block th:focus) {
  box-shadow: inset 0 0 0 2px rgba(79, 124, 255, 0.35);
}
/* 焦点在表格单元格内时：编辑器绘制的光标/选区/当前行按「编辑器光标」绘制，
   会停在表格外误导用户（实测：光标画在第 20 行），必须隐藏。
   用 :has() 直接依据焦点状态判断——EditorView 会改写编辑器根的 class，
   由 JS 维护标记会被它冲掉。 */
.live-host :deep(.cm-editor:has(.live-table-block td:focus) .cm-cursorLayer),
.live-host :deep(.cm-editor:has(.live-table-block th:focus) .cm-cursorLayer),
.live-host :deep(.cm-editor:has(.live-table-block td:focus) .cm-selectionLayer),
.live-host :deep(.cm-editor:has(.live-table-block th:focus) .cm-selectionLayer) {
  display: none !important;
}
.live-host :deep(.cm-editor:has(.live-table-block td:focus) .cm-activeLine),
.live-host :deep(.cm-editor:has(.live-table-block th:focus) .cm-activeLine) {
  background: transparent !important;
}
/* 单元格直接编辑（Typora 式）：单元格自身是编辑宿主，聚焦时给蓝色描边 */
.live-host :deep(.live-table-block td:focus),
.live-host :deep(.live-table-block th:focus) {
  outline: none;
  box-shadow: inset 0 0 0 2px rgba(79, 124, 255, 0.45);
}

/* 正在编辑的单元格：焦点宿主始终是编辑器正文，用选区高亮指示编辑目标 */
.live-host :deep(.live-table-block td.is-editing),
.live-host :deep(.live-table-block th.is-editing) {
  box-shadow: inset 0 0 0 2px rgba(79, 124, 255, 0.4);
}
/* 列宽拖拽手柄（表头单元格右边界） */
.live-host :deep(.tbl-col-resize) {
  position: absolute;
  top: 0;
  right: -3px;
  z-index: 2;
  width: 7px;
  height: 100%;
  cursor: col-resize;
}
.live-host :deep(.live-table-block th:hover .tbl-col-resize) {
  background: rgba(0, 0, 0, 0.12);
}
.live-host :deep(.tbl-col-resize:hover) {
  background: rgba(79, 124, 255, 0.35);
}

/* ── 表格浮动工具条（光标位于表格内时浮现） ── */
.live-host :deep(.tbl-bar) {
  position: absolute;
  z-index: 30;
  display: flex;
  align-items: center;
  gap: 1px;
  padding: 3px 4px;
  border: 1px solid rgba(0, 0, 0, 0.1);
  border-radius: 8px;
  background: rgba(255, 255, 255, 0.97);
  box-shadow: 0 6px 18px rgba(23, 26, 33, 0.13), 0 1px 2px rgba(23, 26, 33, 0.07);
  backdrop-filter: blur(7px);
  animation: tbl-bar-in 130ms cubic-bezier(0.32, 0.72, 0, 1);
}
.live-host :deep(.tbl-bar[hidden]) {
  display: none;
}
@keyframes tbl-bar-in {
  from { opacity: 0; transform: translateY(3px); }
  to { opacity: 1; transform: none; }
}
.live-host :deep(.tbl-sep) {
  flex: none;
  width: 1px;
  height: 16px;
  margin: 0 4px;
  background: rgba(0, 0, 0, 0.06);
}
.live-host :deep(.tbl-btn) {
  position: relative;
  flex: none;
  width: 26px;
  height: 26px;
  padding: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border: none;
  border-radius: 6px;
  background: none;
  color: rgba(46, 50, 56, 0.62);
  cursor: pointer;
  transition: background 90ms ease, color 90ms ease;
}
.live-host :deep(.tbl-btn:hover) {
  background: rgba(0, 0, 0, 0.055);
  color: #2e3238;
}
.live-host :deep(.tbl-btn.is-on) {
  background: rgba(79, 124, 255, 0.12);
  color: #3b63d6;
}
.live-host :deep(.tbl-btn.is-on:hover) {
  background: rgba(79, 124, 255, 0.18);
}
.live-host :deep(.tbl-btn.is-danger:hover) {
  background: rgba(180, 35, 24, 0.1);
  color: #b42318;
}
.live-host :deep(.tbl-btn:disabled) {
  color: rgba(0, 0, 0, 0.2);
  background: none;
  cursor: default;
}
.live-host :deep(.tbl-btn svg) {
  width: 17px;
  height: 17px;
  display: block;
}
/* 工具条 tooltip（比 title 更可控，随工具条浮起） */
.live-host :deep(.tbl-btn::before) {
  content: attr(data-tip);
  position: absolute;
  bottom: calc(100% + 7px);
  left: 50%;
  transform: translateX(-50%) translateY(2px);
  padding: 3px 7px;
  border-radius: 5px;
  white-space: nowrap;
  font-size: 11px;
  line-height: 1.5;
  color: #fff;
  background: rgba(46, 50, 56, 0.94);
  opacity: 0;
  pointer-events: none;
  transition: opacity 90ms ease, transform 90ms ease;
}
.live-host :deep(.tbl-btn:hover::before) {
  opacity: 1;
  transform: translateX(-50%);
}

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
