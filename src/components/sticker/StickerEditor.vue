<script setup lang="ts">
import { ref, onMounted } from "vue";
import { invoke } from "../../composables/useTauri";
import SlashMenu from "../slash/SlashMenu.vue";
import type { SlashItem } from "../../types";

const props = defineProps<{
  content: string;
  stickerId: number;
}>();

const emit = defineEmits<{
  saved: [];
  cancelled: [];
  toggleSettings: [];
}>();

const content = ref(props.content);
const textarea = ref<HTMLTextAreaElement | null>(null);
const slashMenu = ref(false);
const slashItems = ref<SlashItem[]>([]);
const slashQuery = ref("");
const slashSelected = ref(0);

/** 从 markdown 第一行提取标题（`# xxx` → xxx）。 */
function extractTitle(text: string): string {
  const first = text.split("\n").find((l) => l.trim().length > 0) ?? "";
  const trimmed = first.trim();
  if (trimmed.startsWith("# ")) {
    return trimmed.slice(2).trim();
  }
  return trimmed.slice(0, 30);
}

async function querySlash(q: string) {
  if (q.startsWith("/")) {
    slashQuery.value = q.slice(1);
    slashItems.value = await invoke<SlashItem[]>("slash_query_cmd", { query: slashQuery.value });
    slashMenu.value = slashItems.value.length > 0;
    slashSelected.value = 0;
  } else {
    slashMenu.value = false;
  }
}

function onInput() {
  const el = textarea.value;
  if (!el) return;
  querySlash(content.value.slice(0, el.selectionStart));
}

function applySlash(item: SlashItem) {
  const el = textarea.value;
  if (!el) return;
  const caret = el.selectionStart;
  const before = content.value.slice(0, caret);
  const slash = before.lastIndexOf("/");
  const prefix = before.slice(0, slash);
  const after = content.value.slice(caret);
  content.value = prefix + item.template + after;
  slashMenu.value = false;
  requestAnimationFrame(() => {
    const pos = (prefix + item.template).length;
    el.focus();
    el.setSelectionRange(pos, pos);
  });
}

async function save() {
  await invoke("update_sticker_cmd", {
    id: props.stickerId,
    patch: { title: extractTitle(content.value), content: content.value },
  });
  emit("saved");
}

function onKeydown(e: KeyboardEvent) {
  if (slashMenu.value && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
    e.preventDefault();
    const len = slashItems.value.length;
    if (!len) return;
    slashSelected.value =
      (slashSelected.value + (e.key === "ArrowDown" ? 1 : -1) + len) % len;
    return;
  }
  if (slashMenu.value && e.key === "Enter" && slashItems.value[slashSelected.value]) {
    e.preventDefault();
    applySlash(slashItems.value[slashSelected.value]);
    return;
  }
  if (slashMenu.value && e.key === "Escape") {
    slashMenu.value = false;
    return;
  }
  if (e.key === "Tab") {
    e.preventDefault();
    const el = textarea.value;
    if (!el) return;
    const caret = el.selectionStart;
    content.value = content.value.slice(0, caret) + "  " + content.value.slice(caret);
    requestAnimationFrame(() => {
      el.setSelectionRange(caret + 2, caret + 2);
    });
  }
}

onMounted(() => {
  textarea.value?.focus();
});
</script>

<template>
  <div class="editor">
    <textarea
      ref="textarea"
      v-model="content"
      class="content-input"
      placeholder="第一行输入 # 标题，/ 唤起命令菜单"
      @input="onInput"
      @keydown="onKeydown"
      @click="onInput"
    ></textarea>
    <div class="bar">
      <button class="btn primary" @click="save">保存</button>
      <button class="btn" @click="emit('cancelled')">取消</button>
      <button class="btn" title="设置" @click="emit('toggleSettings')">⚙</button>
    </div>
    <SlashMenu
      v-if="slashMenu"
      :items="slashItems"
      :selected="slashSelected"
      @select="applySlash"
      @close="slashMenu = false"
    />
  </div>
</template>

<style scoped>
.editor {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-height: 100%;
  position: relative;
}

.content-input {
  flex: 1;
  min-height: 200px;
  border: none;
  border-radius: 8px;
  padding: 10px;
  font-size: 13px;
  line-height: 1.7;
  font-family: inherit;
  color: #333;
  resize: none;
  outline: none;
  background: rgba(255, 255, 255, 0.55);
}

.content-input:focus {
  box-shadow: 0 0 0 2px rgba(79, 124, 255, 0.4);
}

.bar {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
}

.btn {
  border: 1px solid rgba(0, 0, 0, 0.12);
  border-radius: 8px;
  padding: 6px 14px;
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
</style>
