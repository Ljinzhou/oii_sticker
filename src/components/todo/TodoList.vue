<script setup lang="ts">
import { computed } from "vue";
import type { TodoBlock } from "../../types";

const props = defineProps<{ items: TodoBlock[]; selectedId: string | null; height: number }>();
const emit = defineEmits<{ select: [id: string]; createRoot: []; createChild: [id: string]; toggle: [id: string, checked: boolean]; remove: [id: string] }>();

const roots = computed(() => props.items.filter((item) => !item.parent_id));
const childrenByParent = computed(() => {
  const children = new Map<string, TodoBlock[]>();
  for (const item of props.items) {
    if (!item.parent_id) continue;
    const siblings = children.get(item.parent_id) ?? [];
    siblings.push(item);
    children.set(item.parent_id, siblings);
  }
  return children;
});
</script>

<template>
  <section class="todo-upper" :style="{ height: height + 'px' }">
    <header><strong>任务列表</strong><button @click="emit('createRoot')">+ 新建</button></header>
    <ul class="todo-list">
      <template v-for="item in roots" :key="item.id">
        <li :class="{ selected: item.id === selectedId, done: item.is_completed }" @click="emit('select', item.id)">
          <input class="wb-checkbox" type="checkbox" :checked="item.is_completed" @click.stop @change="emit('toggle', item.id, ($event.target as HTMLInputElement).checked)" />
          <span class="label">{{ item.title || '未命名任务' }}</span>
          <button class="row-delete" title="删除" @click.stop="emit('remove', item.id)">×</button>
        </li>
        <li v-for="child in childrenByParent.get(item.id) ?? []" :key="child.id" class="sub-task" :class="{ selected: child.id === selectedId, done: child.is_completed }" @click="emit('select', child.id)">
          <input class="wb-checkbox" type="checkbox" :checked="child.is_completed" @click.stop @change="emit('toggle', child.id, ($event.target as HTMLInputElement).checked)" />
          <span class="label">{{ child.title || '未命名子任务' }}</span>
          <button class="row-delete" title="删除" @click.stop="emit('remove', child.id)">×</button>
        </li>
        <li class="add-child" @click.stop="emit('createChild', item.id)">└ + 添加子任务</li>
      </template>
    </ul>
  </section>
</template>

<style scoped>
.todo-upper { height: 220px; min-height: 120px; max-height: 420px; overflow: auto; padding: 12px 14px; background: rgba(255,255,255,.55); box-sizing: border-box; }
header { display:flex; justify-content:space-between; align-items:center; font-size:14px; color:#333; margin-bottom:7px; }
header button { border:0; background:rgba(255,255,255,.7); color:#4f7cff; border-radius:6px; padding:4px 8px; font:inherit; font-size:12px; cursor:pointer; }
.todo-list { list-style:none; margin:0; padding:0; }
.todo-list li { display:flex; align-items:center; gap:8px; min-height:28px; padding:5px 8px; box-sizing:border-box; border-radius:6px; cursor:pointer; color:#222; font-size:13px; }
.todo-list li.selected { background:rgba(79,124,255,.12); color:#4f7cff; font-weight:600; box-shadow:inset 3px 0 #4f7cff; }
.todo-list li.done .label { color:#999; text-decoration:line-through; }
.todo-list .label { min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.row-delete { margin-left:auto; flex:none; width:20px; height:20px; border:0; border-radius:5px; background:transparent; color:#bbb; font-size:15px; line-height:18px; cursor:pointer; visibility:hidden; }
.todo-list li:hover .row-delete { visibility:visible; }
.row-delete:hover { color:#d33; background:#ffe3e3; }
.todo-list .sub-task { padding-left:26px; }.sub-task::before { content:"└"; color:#bbb; margin-right:-3px; }.add-child { color:#4f7cff; font-size:12px; padding-left:26px; }
.wb-checkbox { appearance:none; -webkit-appearance:none; width:14px; height:14px; flex:none; margin:0; border:1.2px solid rgba(0,0,0,.18); border-radius:3.5px; background:rgba(255,255,255,.75); box-shadow:inset 0 1px 2px rgba(0,0,0,.04); cursor:pointer; position:relative; }.wb-checkbox:hover { border-color:#4f7cff; }.wb-checkbox:checked { background:#4f7cff; border-color:#4f7cff; }.wb-checkbox:checked::after { content:""; position:absolute; left:4px; top:1px; width:3.5px; height:7px; border:solid #fff; border-width:0 1.5px 1.5px 0; transform:rotate(45deg); }.todo-list li.done .wb-checkbox:checked { background:#bfbfbf; border-color:#bfbfbf; }
</style>
