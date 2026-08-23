<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from "vue";
import type { TodoBlock } from "../../types";

const props = defineProps<{ items: TodoBlock[]; selectedId: string | null; height: number }>();
const emit = defineEmits<{ select: [id: string]; createRoot: []; createChild: [id: string]; toggle: [id: string, checked: boolean]; remove: [id: string]; reorder: [ids: string[]] }>();

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

// ── 拖拽排序（指针事件实现，替代 WebView 下不可靠的 HTML5 DnD）：
//   鼠标按下行 → 移动超过阈值后进入拖拽 → 实时计算指针下的目标行 → 松开提交顺序。
const dragId = ref<string | null>(null);
const dropTargetId = ref<string | null>(null);
const dropAddParentId = ref<string | null>(null);
const moved = ref(false);
const suppressClick = ref(false);
let startX = 0;
let startY = 0;

function onRowMouseDown(id: string, event: MouseEvent) {
  if (event.button !== 0) return;
  const target = event.target as HTMLElement | null;
  if (target && target.closest("input, button")) return;
  dragId.value = id;
  dropTargetId.value = null;
  dropAddParentId.value = null;
  moved.value = false;
  startX = event.clientX;
  startY = event.clientY;
  document.addEventListener("mousemove", onPointerMove);
  document.addEventListener("mouseup", onPointerUp);
}

function onPointerMove(event: MouseEvent) {
  if (!dragId.value) return;
  const dx = event.clientX - startX;
  const dy = event.clientY - startY;
  if (!moved.value && Math.hypot(dx, dy) < 4) return; // 未超过阈值：视为点击选择
  if (!moved.value) {
    moved.value = true;
    document.body.style.userSelect = "none";
  }
  // 指针下方最近的列表行：普通行 → dropTargetId；「添加子任务」行 → dropAddParentId
  let el: Element | null = null;
  try {
    el = document.elementFromPoint?.(event.clientX, event.clientY) ?? null;
  } catch {
    el = null;
  }
  const row = el?.closest<HTMLElement>(".todo-list > li") ?? null;
  if (!row) {
    dropTargetId.value = null;
    dropAddParentId.value = null;
    return;
  }
  if (row.classList.contains("add-child")) {
    dropTargetId.value = null;
    dropAddParentId.value = row.dataset.rootId ?? null;
  } else {
    dropAddParentId.value = null;
    const id = row.dataset.id ?? null;
    dropTargetId.value = id && id !== dragId.value ? id : null;
  }
}

function onPointerUp() {
  document.removeEventListener("mousemove", onPointerMove);
  document.removeEventListener("mouseup", onPointerUp);
  document.body.style.userSelect = "";
  const drag = dragId.value;
  if (drag && moved.value) {
    if (dropAddParentId.value && dropAddParentId.value !== drag) {
      onDropAfterRoot(dropAddParentId.value);
    } else if (dropTargetId.value) {
      onDrop(dropTargetId.value);
    } else {
      endDrag();
    }
    // 拖拽结束的 click 事件不再当作「选中行」
    suppressClick.value = true;
    setTimeout(() => {
      suppressClick.value = false;
    }, 0);
  } else {
    endDrag();
  }
  moved.value = false;
}

function onRowClick(id: string) {
  if (suppressClick.value) {
    suppressClick.value = false;
    return;
  }
  emit("select", id);
}

onBeforeUnmount(() => {
  document.removeEventListener("mousemove", onPointerMove);
  document.removeEventListener("mouseup", onPointerUp);
  document.body.style.userSelect = "";
});

function endDrag() { dragId.value = null; dropTargetId.value = null; dropAddParentId.value = null; }

/** 把 drag 移动到 anchor 所在原始位置（其余元素保序）。 */
function moveTo(ids: string[], drag: string, anchor: string): string[] {
  const from = ids.indexOf(drag);
  const to = ids.indexOf(anchor);
  if (from < 0 || to < 0) return ids;
  const moved = [...ids];
  moved.splice(from, 1);
  moved.splice(to, 0, drag);
  return moved;
}

/** 把根任务移动到某根块之后（拖到子任务行 / "添加子任务"行）。 */
function onDropAfterRoot(parentRootId: string) {
  const drag = dragId.value;
  if (!drag) { endDrag(); return; }
  const rootIds = roots.value.map((item) => item.id);
  const from = rootIds.indexOf(drag);
  const toParent = rootIds.indexOf(parentRootId);
  if (from < 0 || toParent < 0) { endDrag(); return; }
  const moved = [...rootIds];
  moved.splice(from, 1);
  const at = moved.indexOf(parentRootId);
  if (at < 0) { endDrag(); return; }
  moved.splice(at + 1, 0, drag);
  if (JSON.stringify(moved) !== JSON.stringify(rootIds)) emit("reorder", moved);
  endDrag();
}

/** 拖放执行：按被拖任务与目标行的关系计算新位置并提交该分组完整顺序。 */
function onDrop(targetId: string) {
  const drag = dragId.value;
  if (!drag || drag === targetId) { endDrag(); return; }
  const dragItem = props.items.find((item) => item.id === drag);
  const targetItem = props.items.find((item) => item.id === targetId);
  if (!dragItem || !targetItem || dragItem.sticker_id !== targetItem.sticker_id) { endDrag(); return; }

  if (!dragItem.parent_id) {
    // 根任务：拖到子任务行 → 放在其父级根任务之后
    if (targetItem.parent_id) { onDropAfterRoot(targetItem.parent_id); return; }
    const rootIds = roots.value.map((item) => item.id);
    const moved = moveTo(rootIds, drag, targetItem.id);
    if (JSON.stringify(moved) !== JSON.stringify(rootIds)) emit("reorder", moved);
  } else {
    // 子任务仅在同父分组内重排
    if (dragItem.parent_id !== targetItem.parent_id) { endDrag(); return; }
    const ids = (childrenByParent.value.get(dragItem.parent_id!) ?? []).map((item) => item.id);
    const moved = moveTo(ids, drag, targetItem.id);
    if (JSON.stringify(moved) !== JSON.stringify(ids)) emit("reorder", moved);
  }
  endDrag();
}
</script>

<template>
  <section class="todo-upper" :style="{ height: height + 'px' }">
    <header><strong>任务列表</strong><button @click="emit('createRoot')"><i class="ri-add-line"></i>新建</button></header>
    <ul class="todo-list">
      <template v-for="item in roots" :key="item.id">
        <li
          :data-id="item.id"
          :class="{ selected: item.id === selectedId, done: item.is_completed, dragging: dragId === item.id, 'drop-target': dropTargetId === item.id && dragId !== item.id }"
          @mousedown="onRowMouseDown(item.id, $event)"
          @click="onRowClick(item.id)"
        >
          <span class="drag-handle" title="拖动排序"><i class="ri-draggable"></i></span><input class="wb-checkbox" type="checkbox" :checked="item.is_completed" @click.stop @change="emit('toggle', item.id, ($event.target as HTMLInputElement).checked)" />
          <span class="label">{{ item.title || '未命名任务' }}</span>
          <button class="row-delete" title="删除" @click.stop="emit('remove', item.id)"><i class="ri-close-line"></i></button>
        </li>
        <li
          v-for="child in childrenByParent.get(item.id) ?? []"
          :key="child.id"
          class="sub-task"
          :data-id="child.id"
          :class="{ selected: child.id === selectedId, done: child.is_completed, dragging: dragId === child.id, 'drop-target': dropTargetId === child.id && dragId !== child.id }"
          @mousedown="onRowMouseDown(child.id, $event)"
          @click="onRowClick(child.id)"
        >
          <span class="drag-handle" title="拖动排序"><i class="ri-draggable"></i></span><i class="ri-corner-right-down-line sub-corner"></i><input class="wb-checkbox" type="checkbox" :checked="child.is_completed" @click.stop @change="emit('toggle', child.id, ($event.target as HTMLInputElement).checked)" />
          <span class="label">{{ child.title || '未命名子任务' }}</span>
          <button class="row-delete" title="删除" @click.stop="emit('remove', child.id)"><i class="ri-close-line"></i></button>
        </li>
        <li
          class="add-child"
          :data-root-id="item.id"
          :class="{ 'drop-target': dropAddParentId === item.id && dragId !== item.id }"
          @click.stop="emit('createChild', item.id)"
        ><i class="ri-corner-right-down-line"></i>添加子任务</li>
      </template>
    </ul>
  </section>
</template>

<style scoped>
.todo-upper { height: 220px; min-height: 120px; max-height: 420px; overflow: auto; padding: 12px 14px; background: rgba(255,255,255,.55); box-sizing: border-box; }
header { display:flex; justify-content:space-between; align-items:center; font-size:14px; color:#333; margin-bottom:7px; }
header button { border:0; background:rgba(255,255,255,.7); color:#4f7cff; border-radius:6px; padding:4px 8px; font:inherit; font-size:12px; cursor:pointer; display:inline-flex; align-items:center; }
header button .ri { vertical-align:-1px; margin-right:3px; }
.todo-list { list-style:none; margin:0; padding:0; }
.todo-list li { display:flex; align-items:center; gap:8px; min-height:28px; padding:5px 8px; box-sizing:border-box; border-radius:6px; cursor:pointer; color:#222; font-size:13px; }
.todo-list li.selected { background:rgba(79,124,255,.12); color:#4f7cff; font-weight:600; box-shadow:inset 3px 0 #4f7cff; }
.todo-list li.dragging { opacity:.4; }
.todo-list li.drop-target { box-shadow:inset 0 2px 0 #4f7cff; background:rgba(79,124,255,.08); }
.drag-handle { flex:none; width:16px; text-align:center; color:#bbb; font-size:14px; line-height:1; display:inline-flex; align-items:center; justify-content:center; cursor:grab; user-select:none; }
.todo-list li:hover .drag-handle { color:#4f7cff; }
.drag-handle:active { cursor:grabbing; }
.todo-list li.done .label { color:#999; text-decoration:line-through; }
.todo-list .label { min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.row-delete { margin-left:auto; flex:none; width:20px; height:20px; border:0; border-radius:5px; background:transparent; color:#bbb; font-size:15px; line-height:1; display:inline-flex; align-items:center; justify-content:center; cursor:pointer; visibility:hidden; }
.todo-list li:hover .row-delete { visibility:visible; }
.row-delete:hover { color:#d33; background:#ffe3e3; }
.todo-list .sub-task { padding-left:26px; }.sub-corner { flex:none; color:#bbb; font-size:12px; line-height:1; }.todo-list .add-child { display:flex; align-items:center; gap:3px; color:#4f7cff; font-size:12px; padding-left:26px; }
.wb-checkbox { appearance:none; -webkit-appearance:none; width:14px; height:14px; flex:none; margin:0; border:1.2px solid rgba(0,0,0,.18); border-radius:3.5px; background:rgba(255,255,255,.75); box-shadow:inset 0 1px 2px rgba(0,0,0,.04); cursor:pointer; position:relative; }.wb-checkbox:hover { border-color:#4f7cff; }.wb-checkbox:checked { background:#4f7cff; border-color:#4f7cff; }.wb-checkbox:checked::after { content:""; position:absolute; left:4px; top:1px; width:3.5px; height:7px; border:solid #fff; border-width:0 1.5px 1.5px 0; transform:rotate(45deg); }.todo-list li.done .wb-checkbox:checked { background:#bfbfbf; border-color:#bfbfbf; }
</style>
