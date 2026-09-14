<script setup lang="ts">
// 分组树（= 文件夹树）：层级缩进 + 拖拽排序/拖入成为子分组 + 分组颜色 + 操作菜单。
// 自包含：直接读写 notes store，父组件只需传入数据与当前选中项并监听事件。
import { computed, ref } from "vue";
import { useNotesStore } from "../../stores/notes";
import type { Sticker, StickerGroup } from "../../types";

const props = defineProps<{
  groups: StickerGroup[];
  stickers: Sticker[];
  /** 当前选中的分组（null = 未分组；undefined = 未选中任何分组） */
  currentId?: number | null;
}>();

const emit = defineEmits<{
  select: [id: number | null];
  delete: [group: StickerGroup];
  "create-sticker": [groupId: number];
}>();

const notes = useNotesStore();

/** 折叠的分组 id 集合。 */
const collapsed = ref<Set<number>>(new Set());
/** 正在内联新建子分组的分组 id。 */
const creatingUnder = ref<number | null>(null);
const newName = ref("");
/** 正在重命名的分组 id 与草稿。 */
const renamingId = ref<number | null>(null);
const renameDraft = ref("");
/** 正在挑选颜色的分组 id。 */
const colorFor = ref<number | null>(null);
/** 菜单展开的分组 id。 */
const menuFor = ref<number | null>(null);
/** 错误提示（后端中文报错直接展示）。 */
const error = ref("");
let errorTimer: ReturnType<typeof setTimeout> | null = null;

function showError(e: unknown) {
  error.value = String(e);
  if (errorTimer) clearTimeout(errorTimer);
  errorTimer = setTimeout(() => (error.value = ""), 4000);
}

/** 调色板（空串 = 无颜色）。 */
const PALETTE = [
  "#4f7cff", "#2e9e5b", "#f08c1e", "#d33",
  "#8b5cf6", "#0ea5e9", "#eab308", "#ec4899",
  "#14b8a6", "#6b7280", "#b45309", "",
];

interface TreeRow {
  group: StickerGroup;
  depth: number;
  count: number;
}

/** 扁平化树（深度优先，同级按 sort_order 排序；折叠的分组不展开子树）。 */
const rows = computed<TreeRow[]>(() => {
  const out: TreeRow[] = [];
  const walk = (parentId: number | null, depth: number) => {
    for (const group of notes.childrenOf(parentId)) {
      out.push({ group, depth, count: countOf(group.id) });
      if (!collapsed.value.has(group.id)) walk(group.id, depth + 1);
    }
  };
  walk(null, 0);
  return out;
});

function countOf(groupId: number): number {
  const ids = notes.groupAndDescendants(groupId);
  return props.stickers.filter((s) => s.group_id != null && ids.has(s.group_id)).length;
}

function toggle(groupId: number) {
  const next = new Set(collapsed.value);
  if (next.has(groupId)) next.delete(groupId);
  else next.add(groupId);
  collapsed.value = next;
}

function expand(groupId: number) {
  const next = new Set(collapsed.value);
  next.delete(groupId);
  collapsed.value = next;
}

function hasChildren(groupId: number): boolean {
  return props.groups.some((g) => g.parent_id === groupId);
}

// ── 操作 ──
function startCreateChild(groupId: number) {
  menuFor.value = null;
  creatingUnder.value = groupId;
  newName.value = "";
}

async function submitCreateChild() {
  const parentId = creatingUnder.value;
  const name = newName.value.trim();
  if (parentId === null || !name) {
    creatingUnder.value = null;
    return;
  }
  try {
    await notes.createGroup(name, parentId);
    expand(parentId); // 让新建的子分组立即可见
  } catch (e) {
    showError(e);
  } finally {
    creatingUnder.value = null;
    newName.value = "";
  }
}

function startRename(group: StickerGroup) {
  menuFor.value = null;
  renamingId.value = group.id;
  renameDraft.value = group.name;
}

async function submitRename() {
  const id = renamingId.value;
  const name = renameDraft.value.trim();
  if (id === null) return;
  renamingId.value = null;
  if (!name) return;
  try {
    await notes.renameGroup(id, name);
  } catch (e) {
    showError(e);
  }
}

async function pickColor(groupId: number, color: string) {
  colorFor.value = null;
  try {
    await notes.setGroupColor(groupId, color === "" ? null : color);
  } catch (e) {
    showError(e);
  }
}

function requestDelete(group: StickerGroup) {
  menuFor.value = null;
  // 有子分组时后端会拒绝：这里先即时反馈，避免用户以为点了没反应
  if (hasChildren(group.id)) {
    showError(`「${group.name}」下还有子分组，请先移动或删除它们`);
    return;
  }
  emit("delete", group);
}

// ── 拖拽：上/下 = 同级排序，中间 = 成为子分组 ──
const dragId = ref<number | null>(null);
const dropTarget = ref<{ id: number; mode: "before" | "after" | "inside" } | null>(null);

function onDragStart(group: StickerGroup, event: DragEvent) {
  dragId.value = group.id;
  menuFor.value = null;
  colorFor.value = null;
  event.dataTransfer?.setData("text/plain", String(group.id));
  if (event.dataTransfer) event.dataTransfer.effectAllowed = "move";
}

/** 按指针在行内的相对位置决定放置模式（上 27% 前、下 27% 后、中间放入）。 */
function onDragOver(group: StickerGroup, event: DragEvent) {
  if (dragId.value === null || dragId.value === group.id) return;
  event.preventDefault();
  const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
  const ratio = rect.height > 0 ? (event.clientY - rect.top) / rect.height : 0.5;
  const mode = ratio < 0.27 ? "before" : ratio > 0.73 ? "after" : "inside";
  dropTarget.value = { id: group.id, mode };
}

function onDragLeave(group: StickerGroup) {
  if (dropTarget.value?.id === group.id) dropTarget.value = null;
}

async function onDrop(group: StickerGroup, event: DragEvent) {
  event.preventDefault();
  const id = dragId.value;
  const target = dropTarget.value;
  dragId.value = null;
  dropTarget.value = null;
  if (id === null || id === group.id || !target || target.id !== group.id) return;
  try {
    if (target.mode === "inside") {
      await notes.moveGroup(id, group.id);
      expand(group.id);
      return;
    }
    // 同级插入：先移动到目标父级，再按目标位置重排
    const parentId = group.parent_id ?? null;
    await notes.moveGroup(id, parentId);
    const siblings = notes
      .childrenOf(parentId)
      .map((g) => g.id)
      .filter((sid) => sid !== id);
    const index = siblings.indexOf(group.id);
    const at = target.mode === "before" ? index : index + 1;
    siblings.splice(at < 0 ? siblings.length : at, 0, id);
    await notes.reorderGroups(parentId, siblings);
  } catch (e) {
    showError(e);
  }
}

/** 拖到树的空白处 = 移动到顶层末尾。 */
async function onDropToRoot(event: DragEvent) {
  event.preventDefault();
  const id = dragId.value;
  dragId.value = null;
  dropTarget.value = null;
  if (id === null) return;
  try {
    await notes.moveGroup(id, null);
  } catch (e) {
    showError(e);
  }
}
</script>

<template>
  <div class="tree" @dragover.prevent @drop="onDropToRoot">
    <p v-if="error" class="tree-error">{{ error }}</p>
    <p v-if="rows.length === 0" class="tree-empty">还没有分组，点击上方「新建分组」开始。</p>

    <div
      v-for="row in rows"
      :key="row.group.id"
      class="tree-row"
      :class="{
        active: props.currentId === row.group.id,
        dragging: dragId === row.group.id,
        'drop-before': dropTarget?.id === row.group.id && dropTarget.mode === 'before',
        'drop-after': dropTarget?.id === row.group.id && dropTarget.mode === 'after',
        'drop-inside': dropTarget?.id === row.group.id && dropTarget.mode === 'inside',
      }"
      :style="{ paddingLeft: `${6 + row.depth * 18}px` }"
      draggable="true"
      @dragstart="onDragStart(row.group, $event)"
      @dragover="onDragOver(row.group, $event)"
      @dragleave="onDragLeave(row.group)"
      @drop="onDrop(row.group, $event)"
      @click="emit('select', row.group.id)"
    >
      <span class="grip" @click.stop title="拖动调整顺序，拖到分组中间成为子分组">
        <i class="ri-draggable"></i>
      </span>
      <span
        class="caret"
        :class="{ hidden: !hasChildren(row.group.id) && !collapsed.has(row.group.id) }"
        @click.stop="toggle(row.group.id)"
      >
        <i :class="collapsed.has(row.group.id) ? 'ri-arrow-right-s-line' : 'ri-arrow-down-s-line'"></i>
      </span>
      <span class="folder" :style="{ color: row.group.color ?? '#9aa0a6' }">
        <i :class="collapsed.has(row.group.id) ? 'ri-folder-3-line' : 'ri-folder-open-line'"></i>
      </span>

      <input
        v-if="renamingId === row.group.id"
        v-model="renameDraft"
        class="rename-input"
        autofocus
        @click.stop
        @keydown.enter="submitRename"
        @keydown.esc="renamingId = null"
        @blur="submitRename"
      />
      <template v-else>
        <span class="name">{{ row.group.name }}</span>
        <span class="count">{{ row.count }}</span>
      </template>

      <span class="spacer"></span>
      <span v-if="row.group.color" class="swatch" :style="{ background: row.group.color }"></span>

      <span class="menu-wrap" @click.stop>
        <button
          class="icon-btn"
          title="分组操作"
          @click="menuFor = menuFor === row.group.id ? null : row.group.id"
        >
          <i class="ri-more-2-fill"></i>
        </button>
        <div v-if="menuFor === row.group.id" class="dropdown">
          <button @click="menuFor = null; emit('create-sticker', row.group.id)">
            <i class="ri-add-line"></i>新建便签
          </button>
          <button @click="startCreateChild(row.group.id)">
            <i class="ri-folder-add-line"></i>新建子分组
          </button>
          <button @click="startRename(row.group)">
            <i class="ri-edit-line"></i>重命名
          </button>
          <button @click="menuFor = null; colorFor = row.group.id">
            <i class="ri-palette-line"></i>修改样式
          </button>
          <div class="sep"></div>
          <button class="danger" @click="requestDelete(row.group)">
            <i class="ri-delete-bin-line"></i>删除分组
          </button>
        </div>
        <div v-if="colorFor === row.group.id" class="dropdown palette">
          <div class="palette-tip">分组颜色</div>
          <div class="palette-grid">
            <button
              v-for="(color, index) in PALETTE"
              :key="index"
              class="pal"
              :class="{ none: color === '' }"
              :style="color ? { background: color } : {}"
              :title="color || '无颜色'"
              @click="pickColor(row.group.id, color)"
            ></button>
          </div>
        </div>
      </span>
    </div>

    <div v-if="creatingUnder !== null" class="create-row" style="padding-left: 24px">
      <input
        v-model="newName"
        class="rename-input"
        placeholder="子分组名称"
        autofocus
        @keydown.enter="submitCreateChild"
        @keydown.esc="creatingUnder = null"
      />
      <button class="btn tiny primary" @click="submitCreateChild">确定</button>
      <button class="btn tiny" @click="creatingUnder = null">取消</button>
    </div>
  </div>
</template>

<style scoped>
.tree {
  display: grid;
  gap: 2px;
  min-height: 60px;
}
.tree-error {
  margin: 0 0 6px;
  padding: 6px 10px;
  border-radius: 7px;
  font-size: 12px;
  color: #d33;
  background: rgba(221, 51, 51, 0.08);
  border: 1px solid rgba(221, 51, 51, 0.25);
}
.tree-empty {
  margin: 4px 0;
  font-size: 12px;
  color: #9aa0a8;
}
.tree-row {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 8px;
  border-radius: 8px;
  border: 1px solid transparent;
  cursor: pointer;
  position: relative;
}
.tree-row:hover {
  background: rgba(0, 0, 0, 0.03);
}
.tree-row.active {
  background: rgba(79, 124, 255, 0.10);
  border-color: rgba(79, 124, 255, 0.22);
}
.tree-row.dragging {
  opacity: 0.45;
}
/* 拖拽放置指示：上下蓝线 / 中间高亮（成为子分组） */
.tree-row.drop-before::before,
.tree-row.drop-after::after {
  content: "";
  position: absolute;
  left: 8px;
  right: 8px;
  height: 2px;
  border-radius: 2px;
  background: #4f7cff;
}
.tree-row.drop-before::before {
  top: -1px;
}
.tree-row.drop-after::after {
  bottom: -1px;
}
.tree-row.drop-inside {
  background: rgba(79, 124, 255, 0.16);
  border-color: #4f7cff;
}
.grip {
  color: #c2c6cd;
  font-size: 13px;
  cursor: grab;
  opacity: 0;
  transition: opacity 120ms ease;
}
.tree-row:hover .grip {
  opacity: 1;
}
.caret {
  width: 14px;
  font-size: 14px;
  color: #9aa0a8;
  display: grid;
  place-items: center;
}
.caret.hidden {
  visibility: hidden;
}
.folder {
  font-size: 15px;
  display: grid;
  place-items: center;
}
.name {
  font-size: 13.5px;
  font-weight: 600;
  color: #2f3338;
}
.count {
  font-size: 11px;
  color: #9aa0a8;
  background: rgba(0, 0, 0, 0.04);
  border-radius: 20px;
  padding: 0 7px;
  line-height: 17px;
}
.spacer {
  flex: 1;
}
.swatch {
  width: 9px;
  height: 9px;
  border-radius: 3px;
  box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.08);
}
.rename-input {
  flex: 1;
  min-width: 60px;
  border: 1px solid rgba(0, 0, 0, 0.14);
  border-radius: 6px;
  padding: 3px 7px;
  font-size: 13px;
  font-family: inherit;
}
.menu-wrap {
  position: relative;
  display: inline-flex;
}
.icon-btn {
  border: none;
  background: none;
  color: #9aa0a8;
  border-radius: 6px;
  padding: 2px 4px;
  cursor: pointer;
  font-size: 14px;
  line-height: 1;
}
.icon-btn:hover {
  background: rgba(0, 0, 0, 0.06);
  color: #2f3338;
}
.dropdown {
  position: absolute;
  right: 0;
  top: 24px;
  z-index: 12;
  min-width: 168px;
  background: #fff;
  border: 1px solid rgba(0, 0, 0, 0.12);
  border-radius: 9px;
  padding: 4px;
  box-shadow: 0 10px 28px rgba(0, 0, 0, 0.14);
}
.dropdown button {
  display: flex;
  align-items: center;
  gap: 7px;
  width: 100%;
  border: none;
  background: none;
  text-align: left;
  cursor: pointer;
  padding: 6px 9px;
  border-radius: 6px;
  font-size: 12.5px;
  color: #2f3338;
}
.dropdown button:hover {
  background: rgba(0, 0, 0, 0.05);
}
.dropdown button.danger {
  color: #d33;
}
.dropdown .sep {
  height: 1px;
  background: rgba(0, 0, 0, 0.07);
  margin: 4px 6px;
}
.palette {
  min-width: 0;
  padding: 8px;
}
.palette-tip {
  font-size: 11px;
  color: #9aa0a8;
  margin-bottom: 6px;
}
.palette-grid {
  display: grid;
  grid-template-columns: repeat(6, 22px);
  gap: 5px;
}
.pal {
  width: 22px;
  height: 22px;
  border-radius: 6px;
  border: 1px solid rgba(0, 0, 0, 0.1);
  cursor: pointer;
}
.pal.none {
  background: #fff;
  position: relative;
}
.pal.none::after {
  content: "∅";
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  color: #bbb;
  font-size: 11px;
}
.create-row {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 8px;
}
.btn.tiny {
  border: 1px solid rgba(0, 0, 0, 0.12);
  background: #fff;
  color: #2f3338;
  border-radius: 7px;
  padding: 3px 9px;
  font-size: 12px;
  cursor: pointer;
}
.btn.tiny.primary {
  background: #4f7cff;
  border-color: #4f7cff;
  color: #fff;
}
</style>
