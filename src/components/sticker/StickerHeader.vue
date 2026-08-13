<script setup lang="ts">
import type { StickerMode } from "../../types";

defineProps<{
  mode: StickerMode;
  title: string;
}>();

const emit = defineEmits<{
  enterEdit: [];
  toggleSettings: [];
  close: [];
}>();
</script>

<template>
  <div class="header" data-tauri-drag-region>
    <span class="title" data-tauri-drag-region>{{ title || "便签" }}</span>
    <div class="tools">
      <button class="tool" title="编辑（E）" @click="emit('enterEdit')">✎</button>
      <button class="tool" title="设置" @click="emit('toggleSettings')">⚙</button>
      <button class="tool close" title="关闭" @click="emit('close')">✕</button>
    </div>
  </div>
</template>

<style scoped>
.header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  background: rgba(255, 255, 255, 0.92);
  border-bottom: 1px solid rgba(0, 0, 0, 0.08);
  cursor: grab;
  user-select: none;
}

.title {
  font-size: 13px;
  font-weight: 600;
  color: #444;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
}

.tools {
  display: flex;
  gap: 4px;
}

.tool {
  border: none;
  background: none;
  width: 24px;
  height: 24px;
  border-radius: 6px;
  font-size: 13px;
  color: #666;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
}

.tool:hover {
  background: rgba(0, 0, 0, 0.06);
}

.tool.close:hover {
  background: #ffe3e3;
  color: #d33;
}
</style>
