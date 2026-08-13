<script setup lang="ts">
import MarkdownView from "../markdown/MarkdownView.vue";

defineProps<{
  content: string;
  interactive: boolean;
}>();

const emit = defineEmits<{
  toggle: [line: number];
  pointer: [];
}>();
</script>

<template>
  <div class="viewer" @mousemove="emit('pointer')" @click="emit('pointer')">
    <MarkdownView :content="content" :interactive="interactive" @toggle="(l) => emit('toggle', l)" />
  </div>
</template>

<style scoped>
.viewer {
  min-height: 100%;
}
</style>
