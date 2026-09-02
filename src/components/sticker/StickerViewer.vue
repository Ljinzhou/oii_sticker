<script setup lang="ts">
import MarkdownView from "../markdown/MarkdownView.vue";
import type { TodoBlock } from "../../types";
import type { BlockUiState } from "../../utils/block-ui";

defineProps<{
  content: string;
  interactive: boolean;
  todoBlocks?: TodoBlock[];
  /** 用户折叠状态快照（透传 MarkdownView）。 */
  uiState?: BlockUiState;
  /** UI 状态的便签键（stickerId 字符串）。 */
  uiKey?: string;
}>();

const emit = defineEmits<{
  toggle: [line: number];
  pointer: [];
  openTodo: [id: string];
  toggleTodo: [id: string, checked: boolean];
}>();
</script>

<template>
  <div class="viewer" @mousemove="emit('pointer')" @click="emit('pointer')">
    <MarkdownView :content="content" :interactive="interactive" :todo-blocks="todoBlocks" :ui-state="uiState" :ui-key="uiKey" @toggle="(l) => emit('toggle', l)" @open-todo="(id) => emit('openTodo', id)" @toggle-todo="(id, checked) => emit('toggleTodo', id, checked)" />
  </div>
</template>

<style scoped>
.viewer {
  min-height: 100%;
}
</style>
