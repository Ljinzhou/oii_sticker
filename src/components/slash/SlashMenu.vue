<script setup lang="ts">
import type { SlashItem } from "../../types";

defineProps<{
  items: SlashItem[];
  selected: number;
}>();

const emit = defineEmits<{
  select: [item: SlashItem];
  close: [];
}>();

// 分类分组
function categories(items: SlashItem[]): string[] {
  const seen: string[] = [];
  for (const it of items) {
    if (!seen.includes(it.category)) seen.push(it.category);
  }
  return seen;
}
</script>

<template>
  <div class="slash-menu" @mousedown.prevent>
    <template v-for="cat in categories(items)" :key="cat">
      <div class="cat">{{ cat }}</div>
      <button
        v-for="item in items.filter((x) => x.category === cat)"
        :key="item.id"
        class="item"
        :class="{ active: items.indexOf(item) === selected }"
        @click="emit('select', item)"
      >
        <span class="name">{{ item.name }}</span>
        <span class="hint">{{ item.hint }}</span>
      </button>
    </template>
  </div>
</template>

<style scoped>
.slash-menu {
  position: absolute;
  left: 16px;
  top: 52px;
  width: 240px;
  max-height: 280px;
  overflow-y: auto;
  background: #fff;
  border-radius: 10px;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.22);
  padding: 6px;
  z-index: 10;
}

.cat {
  font-size: 11px;
  color: #999;
  padding: 6px 10px 2px;
}

.item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
  border: none;
  background: none;
  border-radius: 7px;
  padding: 7px 10px;
  font-size: 13px;
  color: #333;
  cursor: pointer;
  text-align: left;
}

.item:hover,
.item.active {
  background: rgba(79, 124, 255, 0.1);
  color: #4f7cff;
}

.hint {
  font-size: 11px;
  color: #aaa;
  font-family: Consolas, monospace;
}
</style>
