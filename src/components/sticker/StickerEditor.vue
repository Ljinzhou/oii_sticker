<script setup lang="ts">
// 编辑容器：持有 draft，按全局配置 editor_mode 路由到
// StickerEditorMarkdown（原生文本）或 StickerEditorLive（及时预览）。
// 保存/退出编辑由 StickerWindow overlay 的按钮调用本组件 expose 的 save()。
import { ref, computed, onMounted } from "vue";
import { invoke } from "../../composables/useTauri";
import { useSettingsStore } from "../../stores/settings";
import StickerEditorMarkdown from "./StickerEditorMarkdown.vue";
import StickerEditorLive from "./StickerEditorLive.vue";

const props = defineProps<{
  content: string;
  stickerId: number;
}>();

const emit = defineEmits<{ saved: [] }>();

const settings = useSettingsStore();
const draft = ref(props.content);
const liveRef = ref<InstanceType<typeof StickerEditorLive> | null>(null);

// 编辑模式形态（system_config editor_mode：markdown | live，默认 markdown）
const editorMode = computed(() => settings.get("editor_mode", "markdown"));
const isLive = computed(() => editorMode.value === "live");
// 编辑模式下文字字号（edit_font_size，默认 14）
const editFontSize = computed(() => Number(settings.get("edit_font_size", "14")));
const editFontFamily = computed(() => settings.editFontFamily);
// 是否显示行号（editor_line_numbers，默认关）
const showLineNumbers = computed(() => settings.get("editor_line_numbers", "0") === "1");

/** 从 markdown 第一行提取标题（`# xxx` → xxx）。 */
function extractTitle(text: string): string {
  const first = text.split("\n").find((l) => l.trim().length > 0) ?? "";
  const trimmed = first.trim();
  if (trimmed.startsWith("# ")) {
    return trimmed.slice(2).trim();
  }
  return trimmed.slice(0, 30);
}

/** 保存：Markdown 原文直接落库，退出后进入交互模式才渲染。
 *  及时预览模式下先 flush（防抖窗口内的输入立即回写，不丢内容）。 */
async function save() {
  if (isLive.value) {
    liveRef.value?.flush();
  }
  const title = extractTitle(draft.value);
  try {
    await invoke("update_sticker_cmd", {
      id: props.stickerId,
      patch: { title, content: draft.value },
    });
  } catch (e) {
    console.error("[ui] 保存失败：", e);
    return;
  }
  emit("saved");
}

onMounted(() => {
  settings.refresh();
});

defineExpose({ save });
</script>

<template>
  <div class="editor-root" :style="{ fontFamily: editFontFamily }">
    <StickerEditorMarkdown
      v-if="!isLive"
      v-model="draft"
      :font-size="editFontSize"
      :font-family="editFontFamily"
      :show-line-numbers="showLineNumbers"
    />
    <StickerEditorLive v-else ref="liveRef" v-model="draft" :font-size="editFontSize" :font-family="editFontFamily" @save="save" />
  </div>
</template>

<style scoped>
.editor-root {
  width: 100%;
  height: 100%;
}
</style>
