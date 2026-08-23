<script setup lang="ts">
import { ref, watch } from "vue";
import type { SlashItem } from "../../types";
import type { SlashAnchor } from "./types";

const props = defineProps<{
  items: SlashItem[];
  selected: number;
  recentIds?: string[];
  anchor: SlashAnchor;
}>();

const emit = defineEmits<{
  select: [item: SlashItem];
  close: [];
  hover: [index: number];
}>();

// 分类分组
function categories(items: SlashItem[]): string[] {
  const seen: string[] = [];
  for (const it of items) {
    if (!seen.includes(it.category)) seen.push(it.category);
  }
  return seen;
}

function isFunction(item: SlashItem) {
  return item.category === "功能" || item.id === "todo-block" || item.id === "show-done";
}

function recentItems() {
  return (props.recentIds ?? []).map((id) => props.items.find((item) => item.id === id)).filter((item): item is SlashItem => Boolean(item));
}

/** 键盘上下切换时自动滚动，让高亮项始终可见。 */
const menuRef = ref<HTMLDivElement | null>(null);
function scrollToSelected() {
  const menu = menuRef.value;
  if (!menu || props.selected < 0) return;
  const el = menu.querySelector<HTMLElement>(`[data-index="${props.selected}"]`);
  if (!el) return;
  const menuTop = menu.getBoundingClientRect().top;
  const menuBottom = menu.getBoundingClientRect().bottom;
  const elTop = el.getBoundingClientRect().top;
  const elBottom = el.getBoundingClientRect().bottom;
  if (elTop < menuTop) menu.scrollTop += elTop - menuTop - 4;
  else if (elBottom > menuBottom) menu.scrollTop += elBottom - menuBottom + 4;
}
watch(() => props.selected, scrollToSelected);
watch(() => props.items, scrollToSelected);
</script>

<template>
  <div ref="menuRef" class="slash-menu" :style="{ left: `min(${props.anchor.left}px, calc(100% - 276px))`, top: `${props.anchor.top}px` }" @mousedown.prevent>
    <template v-if="recentItems().length">
      <div class="cat">最近用过的标签</div>
      <button
        v-for="item in recentItems()"
        :key="`recent-${item.id}`"
        class="item"
        :data-index="items.indexOf(item)"
        :class="{ active: items.indexOf(item) === selected }"
        @mouseenter="emit('hover', items.indexOf(item))"
        @click="emit('select', item)"
      ><span class="name">{{ item.name }}</span><span class="hint">{{ item.hint }}</span></button>
      <div class="divider"></div>
    </template>
    <template v-if="items.some(isFunction)">
      <div class="cat cat-fn">功能标签</div>
      <button
        v-for="item in items.filter(isFunction)"
        :key="item.id"
        class="item item-fn"
        :data-index="items.indexOf(item)"
        :class="{ active: items.indexOf(item) === selected }"
        @mouseenter="emit('hover', items.indexOf(item))"
        @click="emit('select', item)"
      >
        <span class="name"><span class="ic-fn"><i :class="item.id === 'todo-block' ? 'ri-checkbox-circle-fill' : 'ri-circle-fill'"></i></span>{{ item.name }}</span><span class="hint">{{ item.hint }}</span>
      </button>
      <div class="divider"></div>
    </template>
    <template v-for="cat in categories(items.filter((item) => !isFunction(item)))" :key="cat">
      <div class="cat">{{ cat }}</div>
      <button
        v-for="item in items.filter((x) => x.category === cat && !isFunction(x))"
        :key="item.id"
        class="item"
        :data-index="items.indexOf(item)"
        :class="{ active: items.indexOf(item) === selected }"
        @mouseenter="emit('hover', items.indexOf(item))"
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
  width: 260px;
  max-height: 360px;
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
.cat-fn { color: #7c3aed; }
.divider { height: 1px; margin: 5px 4px; background: rgba(0,0,0,.08); }

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
  outline: none;
}

.hint {
  font-size: 11px;
  color: #aaa;
  font-family: Consolas, monospace;
}
.name { display:flex; align-items:center; gap:6px; }.ic-fn { color:#7c3aed; font-size:14px; display:inline-flex; }.item-fn:hover,.item-fn.active { color:#7c3aed; }
</style>
