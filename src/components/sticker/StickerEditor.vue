<script setup lang="ts">
// 编辑容器：持有 draft，按全局配置 editor_mode 路由到
// StickerEditorMarkdown（原生文本）或 StickerEditorLive（及时预览）。
// 保存/退出编辑由 StickerWindow overlay 的按钮调用本组件 expose 的 save()。
import { ref, computed, onMounted, watch } from "vue";
import { invoke } from "../../composables/useTauri";
import { useSettingsStore } from "../../stores/settings";
import StickerEditorMarkdown from "./StickerEditorMarkdown.vue";
import StickerEditorLive from "./StickerEditorLive.vue";
import SlashMenu from "../slash/SlashMenu.vue";
import type { SlashItem, TodoBlock } from "../../types";
import type { SlashAnchor } from "../slash/types";
import { extractTitleFromContent } from "../../utils/sticker-title";

const props = defineProps<{
  content: string;
  stickerId: number;
  todoBlocks?: TodoBlock[];
}>();

const emit = defineEmits<{ saved: []; error: [message: string] }>();

const settings = useSettingsStore();
const draft = ref(props.content);
// 用户是否有未保存的编辑（外部内容更新时若为 true 则不做回写，避免覆盖在途输入）
const dirty = ref(false);
const liveRef = ref<InstanceType<typeof StickerEditorLive> | null>(null);

/** 子编辑器写回：记录用户编辑（dirty）并保留在途输入。 */
function onDraftEdit(value: string) {
  dirty.value = true;
  draft.value = value;
}

// 外部内容更新（保存后 load / sticky://push-update / 斜杠插入）→ 同步进编辑器。
// 仅在用户未编辑（未 dirty 且及时预览实例无在途输入）时回写，避免覆盖新输入。
watch(
  () => props.content,
  (value) => {
    if (dirty.value) return;
    if (isLive.value && liveRef.value?.isDirty?.()) return;
    draft.value = value;
  },
);
const slashItems = ref<SlashItem[]>([]);
const slashFrom = ref(0);
const slashTo = ref(0);
const slashSelected = ref(0);
const slashAnchor = ref<SlashAnchor>({ left: 16, top: 52 });
// 斜杠菜单是否打开（编辑内核据此把 ↑/↓/Enter/Esc 让给菜单）
const slashOpen = computed(() => slashItems.value.length > 0);

/** 上下箭头循环切换菜单高亮项。 */
function moveSlashSelection(dir: 1 | -1) {
  const n = slashItems.value.length;
  if (!n) return;
  slashSelected.value = (slashSelected.value + dir + n) % n;
}

/** 回车选中当前高亮项。 */
function confirmSlash() {
  const item = slashItems.value[slashSelected.value];
  if (item) void selectSlash(item);
}

function closeSlash() {
  slashItems.value = [];
  slashSelected.value = 0;
}
const createdTodoIds = new Set<string>();

// 编辑模式形态（system_config editor_mode：markdown | live，默认 markdown）
const editorMode = computed(() => settings.get("editor_mode", "markdown"));
const isLive = computed(() => editorMode.value === "live");
// 编辑模式下文字字号（edit_font_size，默认 14）
const editFontSize = computed(() => Number(settings.get("edit_font_size", "14")));
const editFontFamily = computed(() => settings.editFontFamily);
// 是否显示行号（editor_line_numbers，默认开；及时预览与 Markdown 两种模式共用）
const showLineNumbers = computed(() => settings.get("editor_line_numbers", "1") === "1");

function messageOf(error: unknown): string {
  if (error instanceof Error) return error.message;
  return typeof error === "string" ? error : String(error);
}

/** 后端拒绝“改标题”的原因是否属于命名问题（命名规则 / 同分组重名 / 磁盘已有同名文件）。 */
function isTitleRejection(msg: string): boolean {
  return msg.includes("名称") || msg.includes("同名") || msg.includes("已经存在");
}

/** 保存：Markdown 原文直接落库，退出后进入交互模式才渲染。
 *  及时预览模式下先 flush（防抖窗口内的输入立即回写，不丢内容）。
 *  标题取内容中「第一个一级标题」（可不在第一行）。 */
async function save() {
  if (isLive.value) {
    liveRef.value?.flush();
  }
  // 标题同时是磁盘上的 md 文件名：不合法或与同分组便签重名时不能硬失败，
  // 否则用户改个错别字都会丢掉整篇正文。
  const title = extractTitleFromContent(draft.value);
  try {
    await invoke("update_sticker_cmd", {
      id: props.stickerId,
      patch: { title, content: draft.value },
    });
  } catch (e) {
    const msg = messageOf(e);
    if (!isTitleRejection(msg)) {
      console.error("[ui] 保存失败：", e);
      emit("error", msg);
      return;
    }
    // 降级：正文照存，标题保持原样（改标题请在主控台「重命名」里输合法且不重名的名称）
    try {
      await invoke("update_sticker_cmd", {
        id: props.stickerId,
        patch: { content: draft.value },
      });
    } catch (e2) {
      console.error("[ui] 保存失败：", e2);
      emit("error", messageOf(e2));
      return;
    }
    emit("error", `${msg}（正文已保存，标题未改动）`);
  }
  createdTodoIds.clear();
  dirty.value = false;
  emit("saved");
}

async function discard() {
  await Promise.all([...createdTodoIds].map((id) => invoke("delete_todo_block_cmd", { id })));
  createdTodoIds.clear();
  dirty.value = false;
}

onMounted(() => {
  settings.refresh();
});

async function openSlash(query: string, from: number, to: number, anchor?: SlashAnchor) {
  slashFrom.value = from; slashTo.value = to; slashSelected.value = 0;
  if (anchor) slashAnchor.value = anchor;
  slashItems.value = await invoke<SlashItem[]>("slash_query_cmd", { query });
}

async function openTodo(id: string) {
  try {
    await invoke("open_todo_window_cmd", { id });
  } catch (error) {
    console.error("[editor] 打开 Todo 窗口失败：", error);
  }
}

async function selectSlash(item: SlashItem) {
  if (item.id === "todo-block") {
    const block = await invoke<{ id: string }>("create_todo_block_cmd", { stickerId: props.stickerId, parentId: null });
    createdTodoIds.add(block.id);
    draft.value = `${draft.value.slice(0, slashFrom.value)}<todo-block id="${block.id}"></todo-block>${draft.value.slice(slashTo.value)}`;
  } else if (item.id === "show-done") {
    draft.value = `${draft.value.slice(0, slashFrom.value)}<show-done></show-done>${draft.value.slice(slashTo.value)}`;
  } else {
    draft.value = `${draft.value.slice(0, slashFrom.value)}${item.template}${draft.value.slice(slashTo.value)}`;
    const prior = settings.recentSlashCommands.filter((id) => id !== item.id);
    await settings.set("recent_slash_commands", JSON.stringify([item.id, ...prior].slice(0, 5)));
  }
  slashItems.value = [];
}

defineExpose({ save, discard });
</script>

<template>
  <div class="editor-root" :style="{ fontFamily: editFontFamily }">
    <StickerEditorMarkdown
      v-if="!isLive"
      :model-value="draft"
      @update:model-value="onDraftEdit"
      :sticker-id="props.stickerId"
      :font-size="editFontSize"
      :font-family="editFontFamily"
      :show-line-numbers="showLineNumbers"
      :slash-open="slashOpen"
      @slash="openSlash"
      @slash-close="closeSlash"
      @slash-nav="moveSlashSelection"
      @slash-confirm="confirmSlash"
      @slash-cancel="closeSlash"
      @open-todo="openTodo"
      @error="emit('error', $event)"
    />
    <StickerEditorLive
      v-else
      ref="liveRef"
      :model-value="draft"
      @update:model-value="onDraftEdit"
      :sticker-id="props.stickerId"
      :font-size="editFontSize"
      :font-family="editFontFamily"
      :show-line-numbers="showLineNumbers"
      :todo-blocks="props.todoBlocks ?? []"
      :ui-key="String(props.stickerId)"
      :slash-open="slashOpen"
      @save="save"
      @slash="openSlash"
      @slash-close="closeSlash"
      @slash-nav="moveSlashSelection"
      @slash-confirm="confirmSlash"
      @slash-cancel="closeSlash"
      @open-todo="openTodo"
      @error="emit('error', $event)"
    />
    <SlashMenu v-if="slashItems.length" :items="slashItems" :selected="slashSelected" :recent-ids="settings.recentSlashCommands" :anchor="slashAnchor" @select="selectSlash" @hover="slashSelected = $event" @close="closeSlash" />
  </div>
</template>

<style scoped>
.editor-root {
  width: 100%;
  height: 100%;
  position: relative;
}
</style>
