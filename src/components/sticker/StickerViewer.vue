<script setup lang="ts">
import MarkdownView from "../markdown/MarkdownView.vue";
import type { TodoBlock } from "../../types";

defineProps<{
  content: string;
  interactive: boolean;
  todoBlocks?: TodoBlock[];
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
    <MarkdownView :content="content" :interactive="interactive" :todo-blocks="todoBlocks" @toggle="(l) => emit('toggle', l)" @open-todo="(id) => emit('openTodo', id)" @toggle-todo="(id, checked) => emit('toggleTodo', id, checked)" />
  </div>
</template>

<style scoped>
.viewer {
  min-height: 100%;
}
</style>
