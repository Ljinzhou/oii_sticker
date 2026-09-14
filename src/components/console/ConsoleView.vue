<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useNotesStore } from "../../stores/notes";
import { useSettingsStore } from "../../stores/settings";
import { invoke, listen } from "../../composables/useTauri";
import type { UnlistenFn } from "@tauri-apps/api/event";
import type { NewSticker, Sticker, StickerGroup } from "../../types";
import SettingsPanel from "./SettingsPanel.vue";
import StickerCard from "./StickerCard.vue";
import StickerPreview from "./StickerPreview.vue";
import TodoOverviewView from "./TodoOverviewView.vue";

const notes = useNotesStore();
const settings = useSettingsStore();

/** 主控台背景透明度（系统设置 → 通用 → 主控台背景透明度；默认 0.94） */
const consoleBgAlpha = computed(() => {
  const v = Number(settings.get("console_bg_opacity", "94"));
  if (!Number.isFinite(v)) return 0.94;
  return Math.min(1, Math.max(0.3, v / 100));
});
const showSettings = ref(false);
const openIds = ref<number[]>([]);
const confirming = ref<Sticker | null>(null);
const unlisteners: UnlistenFn[] = [];

const newSticker = (groupId: number | null = null): NewSticker => ({
  title: "新建便签",
  content: "# 标题\n\n在这里写内容...",
  pos_x: 200,
  pos_y: 150,
  width: 400,
  height: 500,
  opacity: settings.opacity,
  bg_color: settings.bgColor,
  always_on_top: settings.get("default_sticker_always_on_top", "1") === "1",
  auto_scroll: false,
  group_id: groupId,
});

async function createSticker() {
  await createStickerInGroup(null);
}

/** 为指定分组（null = 未分组）创建新便签。 */
async function createStickerInGroup(groupId: number | null) {
  try {
    await notes.create(newSticker(groupId));
  } catch (e) {
    console.error("[ui] 新建便签失败：", e);
  }
}

async function removeSticker(s: Sticker) {
  await notes.remove(s.id);
  confirming.value = null;
}

/** 隐藏/显示便签窗口切换（数据保留）。 */
async function toggleSticker(s: Sticker) {
  if (isOpen(s.id)) {
    await invoke("hide_sticker_cmd", { id: s.id });
  } else {
    await invoke("wake_sticker_cmd", { id: s.id });
  }
  await refreshOpenIds();
}

/** 重置便签窗口大小与位置：恢复默认 400×500 并居中到当前显示器；展示模式保持尺寸锁定 */
async function resetStickerWindow(s: Sticker) {
  await invoke("reset_sticker_window_cmd", {
    id: s.id,
    isDisplay: s.display_mode === "display",
  });
  await refreshOpenIds();
}

function isOpen(id: number): boolean {
  return openIds.value.includes(id);
}

async function refreshOpenIds() {
  openIds.value = await invoke<number[]>("list_open_sticker_ids_cmd");
}

function minimizeWindow() {
  getCurrentWindow().minimize();
}

function closeWindow() {
  // 按设置行为关闭：隐藏到托盘或退出程序（Rust 侧处理，无前端权限问题）
  invoke("main_close_cmd");
}

// —— 窗口最大化 / 还原（按 tauri 窗口状态）；测试环境无该 API 时静默降级 ——
const isMaximized = ref(false);
async function toggleMaximizeWindow() {
  try {
    const win = getCurrentWindow();
    await win.toggleMaximize();
    isMaximized.value = await win.isMaximized();
  } catch (e) {
    console.warn("[ui] 最大化窗口失败：", e);
  }
}
async function refreshMaximized() {
  try {
    isMaximized.value = await getCurrentWindow().isMaximized();
  } catch {
    /* 测试环境 / 无权限：忽略 */
  }
}

// —— 视图模式（持久化 system_config；启动时先默认分区，待 settings 回读后恢复） ——
const viewMode = ref<"section" | "flat">("section");
function setViewMode(m: "section" | "flat") {
  viewMode.value = m;
  void settings.set("console_group_view", m);
}

// —— 页面切换（便签 / 任务总览，持久化 system_config.console_page） ——
const consolePage = ref<"stickers" | "todos">("stickers");
function setConsolePage(page: "stickers" | "todos") {
  consolePage.value = page;
  void settings.set("console_page", page);
}
/** 「打开便签」：唤起便签窗口（隐藏中时重建显示）。 */
async function openSticker(stickerId: number) {
  await invoke("wake_sticker_cmd", { id: stickerId });
  showGroupToast("已打开便签窗口");
}

type Section = {
  key: string;
  name: string;
  isDefault: boolean;
  groupId: number | null;
  stickers: Sticker[];
  /** 树形缩进层级（0 = 顶层） */
  depth: number;
  /** 分组颜色（未分组为 null） */
  color: string | null;
  /** 含子分组的便签数 */
  total: number;
};

/** 分组树（深度优先）；「未分组」仅在存在未分组便签时排在最前。 */
const groupSections = computed<Section[]>(() => {
  const ungrouped = notes.stickers.filter((s) => s.group_id == null);
  const sections: Section[] = [];
  if (ungrouped.length > 0) {
    sections.push({
      key: "default",
      name: "未分组",
      isDefault: true,
      groupId: null,
      stickers: ungrouped,
      depth: 0,
      color: null,
      total: ungrouped.length,
    });
  }
  const walk = (parentId: number | null, depth: number) => {
    for (const g of notes.childrenOf(parentId)) {
      const ids = notes.groupAndDescendants(g.id);
      const all = notes.stickers.filter((s) => s.group_id != null && ids.has(s.group_id));
      sections.push({
        key: String(g.id),
        name: g.name,
        isDefault: false,
        groupId: g.id,
        // 分组块内展示「本级」便签；子分组各有自己的块（树形视图）
        stickers: notes.stickers.filter((s) => s.group_id === g.id),
        depth,
        color: g.color ?? null,
        total: all.length,
      });
      walk(g.id, depth + 1);
    }
  };
  walk(null, 0);
  return sections;
});

// 折叠状态（会话级，不持久化）
const collapsed = ref<Record<string, boolean>>({});
function toggleCollapse(key: string) {
  collapsed.value[key] = !collapsed.value[key];
}

// 平铺筛选（分行渲染见 flatRows；选中某分组时只保留该行）
const filter = ref<"all" | "default" | number>("all");

/** 平铺视图按文件夹分行：每个顶层分组一行（行内含其子文件夹的便签），未分组单独一行。 */
interface FlatRow {
  key: string;
  name: string;
  path: string;
  color: string | null;
  isDefault: boolean;
  stickers: Sticker[];
}
const flatRows = computed<FlatRow[]>(() => {
  const rows: FlatRow[] = [];
  const pushGroup = (g: StickerGroup) => {
    const ids = notes.groupAndDescendants(g.id);
    const list = notes.stickers.filter((s) => s.group_id != null && ids.has(s.group_id));
    if (filter.value !== "all" && filter.value !== g.id) return;
    rows.push({
      key: String(g.id),
      name: g.name,
      path: `stickers/${notes.groupPath(g.id)}`,
      color: g.color ?? null,
      isDefault: false,
      stickers: list,
    });
  };
  const ungrouped = notes.stickers.filter((s) => s.group_id == null);
  if (ungrouped.length > 0 && (filter.value === "all" || filter.value === "default")) {
    rows.push({
      key: "default",
      name: "未分组",
      path: "stickers/（根目录）",
      color: null,
      isDefault: true,
      stickers: ungrouped,
    });
  }
  for (const g of notes.topLevelGroups()) pushGroup(g);
  return rows;
});

// 分组操作
const creatingGroup = ref(false);
const newGroupName = ref("");
async function onCreateGroup() {
  const name = newGroupName.value.trim();
  if (!name) return;
  await notes.createGroup(name, null);
  newGroupName.value = "";
  creatingGroup.value = false;
}
/** 在某个分组下新建子分组（菜单入口）。 */
const creatingChildUnder = ref<number | null>(null);
const newChildName = ref("");
function startCreateChildGroup(groupId: number) {
  groupMenuFor.value = null;
  creatingChildUnder.value = groupId;
  newChildName.value = "";
}
async function onCreateChildGroup() {
  const parentId = creatingChildUnder.value;
  const name = newChildName.value.trim();
  if (parentId == null || !name) {
    creatingChildUnder.value = null;
    return;
  }
  try {
    await notes.createGroup(name, parentId);
    collapsed.value[String(parentId)] = false; // 展开父级，新子分组立即可见
  } catch (e) {
    showGroupToast(String(e));
  } finally {
    creatingChildUnder.value = null;
    newChildName.value = "";
  }
}
const renamingGroup = ref<number | null>(null);
const groupNameDraft = ref("");
function startRenameGroup(g: { id: number; name: string }) {
  renamingGroup.value = g.id;
  groupNameDraft.value = g.name;
}
async function commitRenameGroup() {
  if (renamingGroup.value == null) return;
  const name = groupNameDraft.value.trim();
  if (!name) {
    renamingGroup.value = null;
    return;
  }
  await notes.renameGroup(renamingGroup.value, name);
  renamingGroup.value = null;
}

// 分组颜色（「修改样式」）
const colorFor = ref<number | null>(null);
const PALETTE = [
  "#4f7cff", "#2e9e5b", "#f08c1e", "#d33",
  "#8b5cf6", "#0ea5e9", "#eab308", "#ec4899",
  "#14b8a6", "#6b7280", "#b45309", "",
];
async function setGroupColor(groupId: number, color: string) {
  colorFor.value = null;
  try {
    await notes.setGroupColor(groupId, color === "" ? null : color);
  } catch (e) {
    showGroupToast(String(e));
  }
}

// 删除分组三选确认框
const deletingGroup = ref<{ id: number; name: string; count: number } | null>(null);
const deleteChoice = ref<"to-default" | "with-stickers">("to-default");
const confirmingWithStickers = ref(false);
function requestDeleteGroup(g: { id: number; name: string; count: number }) {
  groupMenuFor.value = null;
  // 有子分组时后端会拒绝：先即时反馈，避免用户以为点了没反应
  if (notes.childrenOf(g.id).length > 0) {
    showGroupToast(`「${g.name}」下还有子分组，请先移动或删除它们`);
    return;
  }
  deletingGroup.value = g;
  deleteChoice.value = "to-default";
  confirmingWithStickers.value = false;
}
async function onDeleteGroupConfirmed() {
  if (!deletingGroup.value) return;
  const { id } = deletingGroup.value;
  const choice = deleteChoice.value;
  if (choice === "with-stickers" && !confirmingWithStickers.value) {
    confirmingWithStickers.value = true; // 第一次点「连带删除」进入二次确认态
    return;
  }
  const removed = await notes.deleteGroup(id, choice);
  if (choice === "with-stickers") showGroupToast(`已删除分组及其内 ${removed} 张便签`);
  else showGroupToast("分组已删除，便签已移到未分组");
  deletingGroup.value = null;
  confirmingWithStickers.value = false;
}

// 组菜单（标题条 ⋯）
const groupMenuFor = ref<string | null>(null);

// —— 分组拖拽排序（上/下插入到同级；拖到分组上成为子分组） ——
const dragGroupId = ref<number | null>(null);
const dropHint = ref<{ key: string; mode: "before" | "after" | "inside" } | null>(null);

function onGroupDragStart(sec: Section, event: DragEvent) {
  if (sec.isDefault || sec.groupId == null) return;
  dragGroupId.value = sec.groupId;
  groupMenuFor.value = null;
  event.dataTransfer?.setData("text/plain", String(sec.groupId));
  if (event.dataTransfer) event.dataTransfer.effectAllowed = "move";
}

function onGroupDragOver(sec: Section, event: DragEvent) {
  if (dragGroupId.value === null || sec.groupId === dragGroupId.value) return;
  event.preventDefault();
  const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
  const ratio = rect.height > 0 ? (event.clientY - rect.top) / rect.height : 0.5;
  const mode = ratio < 0.27 ? "before" : ratio > 0.73 ? "after" : "inside";
  dropHint.value = { key: sec.key, mode };
}

function onGroupDragEnd() {
  dragGroupId.value = null;
  dropHint.value = null;
}

async function onGroupDrop(sec: Section, event: DragEvent) {
  event.preventDefault();
  const id = dragGroupId.value;
  const hint = dropHint.value;
  dragGroupId.value = null;
  dropHint.value = null;
  if (id === null || hint === null || hint.key !== sec.key || sec.groupId === id) return;
  try {
    if (hint.mode === "inside" && sec.groupId != null) {
      await notes.moveGroup(id, sec.groupId);
      collapsed.value[String(sec.groupId)] = false;
      return;
    }
    const parentId = notes.groups.find((g) => g.id === id)?.parent_id ?? null;
    await notes.moveGroup(id, parentId);
    const siblings = notes
      .childrenOf(parentId)
      .map((g) => g.id)
      .filter((sid) => sid !== id);
    const index = sec.groupId != null ? siblings.indexOf(sec.groupId) : siblings.length - 1;
    const at = hint.mode === "before" ? index : index + 1;
    siblings.splice(at < 0 ? siblings.length : at, 0, id);
    await notes.reorderGroups(parentId, siblings);
  } catch (e) {
    showGroupToast(String(e));
  }
}

// —— 右侧预览：悬停卡片看内容；点击分组看该文件夹概览 ——
const previewSticker = ref<Sticker | null>(null);
const selectedGroupKey = ref<string | null>(null);
const selectedGroup = computed(() => {
  const key = selectedGroupKey.value;
  if (key === null || key === "default") return null;
  return notes.groups.find((g) => String(g.id) === key) ?? null;
});
const selectedGroupStickers = computed(() => {
  const g = selectedGroup.value;
  if (g) {
    const ids = notes.groupAndDescendants(g.id);
    return notes.stickers.filter((s) => s.group_id != null && ids.has(s.group_id));
  }
  if (selectedGroupKey.value === "default") return notes.stickers.filter((s) => s.group_id == null);
  return [];
});

// —— 预览区宽度（可拖拽；拖到最右收起） ——
const paneWidth = ref(340);
const previewCollapsed = ref(false);
const splitEl = ref<HTMLElement | null>(null);
let draggingGutter = false;

function onGutterDown(event: MouseEvent) {
  draggingGutter = true;
  event.preventDefault();
  document.addEventListener("mousemove", onGutterMove);
  document.addEventListener("mouseup", onGutterUp);
}
function onGutterMove(event: MouseEvent) {
  if (!draggingGutter || !splitEl.value) return;
  const rect = splitEl.value.getBoundingClientRect();
  const next = rect.right - event.clientX;
  if (next < 120) {
    // 拖到最右：收起预览（右缘留出「展开预览」手柄）
    previewCollapsed.value = true;
    paneWidth.value = 340;
    return;
  }
  previewCollapsed.value = false;
  paneWidth.value = Math.max(220, Math.min(next, rect.width - 200));
}
function onGutterUp() {
  draggingGutter = false;
  document.removeEventListener("mousemove", onGutterMove);
  document.removeEventListener("mouseup", onGutterUp);
}
function expandPreview() {
  previewCollapsed.value = false;
  paneWidth.value = 340;
}

// 简易 toast（复用 WorkspaceManager 模式）
const groupToast = ref<string | null>(null);
let toastTimer: ReturnType<typeof setTimeout> | null = null;
function showGroupToast(text: string) {
  if (toastTimer) clearTimeout(toastTimer);
  groupToast.value = text;
  toastTimer = setTimeout(() => (groupToast.value = null), 3000);
}

/** 选择分组（点击分组头）：折叠切换 + 右侧显示该文件夹概览。 */
function selectGroup(sec: Section) {
  selectedGroupKey.value = sec.key;
  toggleCollapse(sec.key);
}

onMounted(async () => {
  notes.refresh();
  await settings.refresh(); // 配置回读完成后，再恢复持久化的视图模式
  viewMode.value =
    settings.get("console_group_view", "section") === "flat" ? "flat" : "section";
  consolePage.value = settings.get("console_page", "stickers") === "todos" ? "todos" : "stickers";
  refreshOpenIds();
  refreshMaximized();
  // 后端推送 → 刷新列表 + 窗口打开状态（隐藏/显示按钮实时同步）
  unlisteners.push(
    await listen("sticky://push-update", () => {
      notes.refresh();
      refreshOpenIds();
    }),
  );
  unlisteners.push(await listen("sticky://prefs-updated", () => settings.refresh()));
  // 托盘"系统设置"→ 打开设置面板
  unlisteners.push(await listen("sticky://open-settings", () => (showSettings.value = true)));
});

onBeforeUnmount(() => {
  unlisteners.forEach((u) => u());
  if (toastTimer) clearTimeout(toastTimer);
  document.removeEventListener("mousemove", onGutterMove);
  document.removeEventListener("mouseup", onGutterUp);
});
</script>

<template>
  <main class="console" :style="{ '--console-alpha': consoleBgAlpha, background: `rgba(255, 255, 255, ${consoleBgAlpha})` }">
    <header class="console-header" data-tauri-drag-region>
      <h1>oii_sticker 主控台</h1>
      <div class="view-switch page-switch" role="tablist">
        <button :class="{ on: consolePage === 'stickers' }" @click="setConsolePage('stickers')">
          <i class="ri-sticky-note-line"></i>便签
        </button>
        <button :class="{ on: consolePage === 'todos' }" @click="setConsolePage('todos')">
          <i class="ri-todo-line"></i>任务总览
        </button>
      </div>
      <div class="actions">
        <button class="btn primary" @click="createSticker"><i class="ri-add-line"></i>新建便签</button>
        <button class="btn" @click="showSettings = true"><i class="ri-settings-3-line"></i>系统设置</button>
        <span class="win-ctl">
          <button class="btn ctl" title="最小化" @click="minimizeWindow"><i class="ri-subtract-line"></i></button>
          <button
            class="btn ctl"
            :title="isMaximized ? '向下还原' : '最大化'"
            @click="toggleMaximizeWindow"
          >
            <i :class="isMaximized ? 'ri-file-copy-2-line' : 'ri-checkbox-blank-line'"></i>
          </button>
          <button class="btn ctl close" title="关闭" @click="closeWindow"><i class="ri-close-line"></i></button>
        </span>
      </div>
    </header>

    <!-- 任务总览页 -->
    <TodoOverviewView v-if="consolePage === 'todos'" class="todo-page" @open-sticker="openSticker" />

    <!-- 便签页 -->
    <section v-else class="list">
      <!-- 视图切换 + 新建分组 -->
      <div class="list-toolbar">
        <div class="view-switch" role="tablist">
          <button :class="{ on: viewMode === 'section' }" @click="setViewMode('section')">分区</button>
          <button :class="{ on: viewMode === 'flat' }" @click="setViewMode('flat')">平铺</button>
        </div>
        <div v-if="viewMode === 'section'" class="group-create">
          <template v-if="creatingGroup">
            <input
              v-model="newGroupName"
              class="group-create-input"
              placeholder="分组名称"
              autofocus
              @keydown.enter="onCreateGroup"
              @keydown.esc="creatingGroup = false"
            />
            <button class="btn small primary" @click="onCreateGroup">确定</button>
            <button class="btn small" @click="creatingGroup = false">取消</button>
          </template>
          <button v-else class="btn small" @click="creatingGroup = true"><i class="ri-add-line"></i>新建分组</button>
        </div>
      </div>

      <!-- 分区视图：左（文件夹树 + 便签）/ 中（分隔线）/ 右（预览） -->
      <div v-if="viewMode === 'section'" ref="splitEl" class="section-split" :class="{ collapsed: previewCollapsed }">
        <div class="section-main">
          <div v-for="sec in groupSections" :key="sec.key" class="group-block">
            <header
              class="group-head"
              :class="{
                'group-sub': sec.depth > 0,
                dragging: dragGroupId === sec.groupId,
                'drop-before': dropHint?.key === sec.key && dropHint.mode === 'before',
                'drop-after': dropHint?.key === sec.key && dropHint.mode === 'after',
                'drop-inside': dropHint?.key === sec.key && dropHint.mode === 'inside',
              }"
              :style="{
                marginLeft: `${sec.depth * 16}px`,
                background: sec.color ? `color-mix(in srgb, ${sec.color} 15%, #ffffff)` : undefined,
              }"
              :draggable="!sec.isDefault"
              @click="selectGroup(sec)"
              @dragstart="onGroupDragStart(sec, $event)"
              @dragover="onGroupDragOver(sec, $event)"
              @dragend="onGroupDragEnd"
              @drop="onGroupDrop(sec, $event)"
            >
              <span v-if="!sec.isDefault" class="grip" title="拖动调整顺序，拖到分组中间成为子分组" @click.stop>
                <i class="ri-draggable"></i>
              </span>
              <span class="caret"><i :class="collapsed[sec.key] ? 'ri-arrow-right-s-line' : 'ri-arrow-down-s-line'"></i></span>
              <span class="folder">
                <i :class="collapsed[sec.key] ? 'ri-folder-3-line' : 'ri-folder-open-line'"></i>
              </span>
              <input
                v-if="renamingGroup != null && renamingGroup === sec.groupId"
                v-model="groupNameDraft"
                class="group-rename"
                @click.stop
                @keydown.enter="commitRenameGroup"
                @keydown.esc="renamingGroup = null"
                @blur="commitRenameGroup"
              />
              <span v-else class="group-name">{{ sec.name }}</span>
              <span v-if="sec.color" class="group-swatch" :style="{ background: sec.color }"></span>
              <span class="group-count">{{ sec.total }}</span>
              <button
                v-if="!sec.isDefault"
                class="btn small group-menu-btn"
                title="分组操作"
                @click.stop="groupMenuFor = groupMenuFor === sec.key ? null : sec.key"
              >
                <i class="ri-more-2-fill"></i>
              </button>
              <div v-if="!sec.isDefault && groupMenuFor === sec.key" class="dropdown" @click.stop>
                <button @click="createStickerInGroup(sec.groupId); groupMenuFor = null">
                  <i class="ri-add-line"></i>新建便签
                </button>
                <button @click="startCreateChildGroup(sec.groupId!)">
                  <i class="ri-folder-add-line"></i>新建子分组
                </button>
                <button @click="startRenameGroup({ id: sec.groupId!, name: sec.name }); groupMenuFor = null">
                  <i class="ri-edit-line"></i>重命名
                </button>
                <button @click="groupMenuFor = null; colorFor = sec.groupId">
                  <i class="ri-palette-line"></i>修改样式
                </button>
                <button @click="groupMenuFor = null; notes.openGroupInExplorer(sec.groupId!)">
                  <i class="ri-folder-open-line"></i>在资源管理器打开
                </button>
                <div class="dropdown-sep"></div>
                <button
                  class="danger-item"
                  @click="requestDeleteGroup({ id: sec.groupId!, name: sec.name, count: sec.total })"
                >
                  <i class="ri-delete-bin-line"></i>删除分组
                </button>
              </div>
              <!-- 「修改样式」调色板 -->
              <div v-if="!sec.isDefault && colorFor === sec.groupId" class="dropdown palette" @click.stop>
                <div class="palette-tip">分组背景色</div>
                <div class="palette-grid">
                  <button
                    v-for="(color, index) in PALETTE"
                    :key="index"
                    class="pal"
                    :class="{ none: color === '' }"
                    :style="color ? { background: color } : {}"
                    :title="color || '无颜色'"
                    @click="setGroupColor(sec.groupId!, color)"
                  ></button>
                </div>
              </div>
            </header>

            <!-- 子分组内联新建 -->
            <div
              v-if="creatingChildUnder === sec.groupId"
              class="child-create"
              :style="{ marginLeft: `${(sec.depth + 1) * 16 + 12}px` }"
            >
              <input
                v-model="newChildName"
                class="group-create-input"
                placeholder="子分组名称"
                autofocus
                @keydown.enter="onCreateChildGroup"
                @keydown.esc="creatingChildUnder = null"
              />
              <button class="btn small primary" @click="onCreateChildGroup">确定</button>
              <button class="btn small" @click="creatingChildUnder = null">取消</button>
            </div>

            <div v-show="!collapsed[sec.key]" class="cards">
              <p v-if="sec.stickers.length === 0" class="group-empty">
                {{ sec.isDefault ? "未分组暂无便签" : "此分组暂无便签" }}
              </p>
              <div
                v-for="s in sec.stickers"
                :key="s.id"
                class="card-cell"
                @mouseenter="previewSticker = s"
              >
                <StickerCard
                  :sticker="s"
                  :is-open="isOpen(s.id)"
                  @toggle="toggleSticker"
                  @remove="confirming = $event"
                  @reset-window="resetStickerWindow"
                />
              </div>
            </div>
          </div>
        </div>

        <!-- 分隔线：拖动调整预览宽度；拖到最右收起预览 -->
        <div class="gutter" title="拖动调整预览宽度 / 拖到最右收起" @mousedown="onGutterDown"></div>

        <div class="section-preview" :style="{ width: `${paneWidth}px` }">
          <StickerPreview
            :sticker="previewSticker"
            :group="selectedGroup"
            :group-selected="selectedGroupKey !== null"
            :group-stickers="selectedGroupStickers"
            @edit="toggleSticker"
            @remove="confirming = $event"
          />
        </div>

        <button v-if="previewCollapsed" class="preview-stub" title="展开预览" @click="expandPreview">
          <i class="ri-arrow-left-s-line"></i>展开预览
        </button>
      </div>

      <!-- 平铺视图：按文件夹分行 -->
      <template v-else>
        <div class="filter-chips">
          <button :class="{ on: filter === 'all' }" @click="filter = 'all'">
            全部 {{ notes.stickers.length }}
          </button>
          <button
            v-for="sec in groupSections"
            :key="sec.key"
            :class="{ on: filter === (sec.isDefault ? 'default' : sec.groupId) }"
            @click="filter = sec.isDefault ? 'default' : sec.groupId!"
          >
            {{ sec.name }} {{ sec.total }}
          </button>
        </div>
        <div v-for="row in flatRows" :key="row.key" class="flat-row" :style="{ borderLeftColor: row.color ?? '#c9ccd3' }">
          <div class="flat-head">
            <span class="folder">
              <i class="ri-folder-open-line"></i>
            </span>
            <span class="flat-name">{{ row.name }}</span>
            <span class="group-count">{{ row.stickers.length }}</span>
            <span class="flat-path">{{ row.path }}</span>
          </div>
          <div class="cards">
            <p v-if="row.stickers.length === 0" class="group-empty">该文件夹暂无便签</p>
            <div
              v-for="s in row.stickers"
              :key="s.id"
              class="card-cell"
              @mouseenter="previewSticker = s"
            >
              <StickerCard
                :sticker="s"
                :is-open="isOpen(s.id)"
                @toggle="toggleSticker"
                @remove="confirming = $event"
                @reset-window="resetStickerWindow"
              />
            </div>
          </div>
        </div>
        <p v-if="flatRows.length === 0" class="empty">
          {{ filter === "all" ? '暂无便签，点击"新建便签"开始' : "没有符合条件的便签" }}
        </p>
      </template>
    </section>

    <SettingsPanel v-if="showSettings" @close="showSettings = false" />

    <!-- 删除便签二次确认 -->
    <div v-if="confirming" class="confirm-mask" @click.self="confirming = null">
      <div class="confirm-box">
        <h3>删除便签</h3>
        <p>确定要删除「{{ confirming.title || "（无标题）" }}」吗？<br />删除后数据不可恢复。</p>
        <div class="confirm-actions">
          <button class="btn" @click="confirming = null">取消</button>
          <button class="btn danger" @click="removeSticker(confirming)">确认删除</button>
        </div>
      </div>
    </div>

    <!-- 删除分组三选确认 -->
    <div v-if="deletingGroup" class="confirm-mask" @click.self="deletingGroup = null">
      <div class="confirm-box">
        <h3>删除分组「{{ deletingGroup.name }}」</h3>
        <p>该分组内有 {{ deletingGroup.count }} 张便签。</p>
        <label class="choice">
          <input v-model="deleteChoice" type="radio" value="to-default" />
          <span>移到未分组（便签保留）</span>
        </label>
        <label class="choice">
          <input v-model="deleteChoice" type="radio" value="with-stickers" />
          <span>连同便签一起删除</span>
        </label>
        <p v-if="confirmingWithStickers && deleteChoice === 'with-stickers'" class="warn-line">
          <i class="ri-alert-fill warn-icon"></i>将永久删除这 {{ deletingGroup.count }} 张便签，不可恢复。再次点击「确认」执行。
        </p>
        <div class="confirm-actions">
          <button class="btn" @click="deletingGroup = null; confirmingWithStickers = false">取消</button>
          <button class="btn danger" @click="onDeleteGroupConfirmed">
            {{ deleteChoice === "with-stickers" && confirmingWithStickers ? "确认永久删除" : "确认" }}
          </button>
        </div>
      </div>
    </div>

    <div v-if="groupToast" class="group-toast">{{ groupToast }}</div>
  </main>
</template>

<style scoped>
.console {
  height: 100vh;
  box-sizing: border-box;
  background: rgba(255, 255, 255, 0.94);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.18);
}

.win-ctl {
  display: inline-flex;
  gap: 4px;
  margin-left: 4px;
  padding-left: 8px;
  border-left: 1px solid rgba(0, 0, 0, 0.08);
}

.btn.ctl {
  padding: 3px 9px;
  font-size: 13px;
  line-height: 1.2;
}

.btn.ctl.close:hover {
  background: #ffe3e3;
  color: #d33;
}

.console-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 14px 18px;
  border-bottom: 1px solid rgba(0, 0, 0, 0.08);
  cursor: grab;
}

.console-header h1 {
  margin: 0;
  font-size: 18px;
  color: #333;
}

.actions {
  display: flex;
  gap: 8px;
}

.btn {
  border: 1px solid rgba(0, 0, 0, 0.12);
  border-radius: 8px;
  padding: 7px 12px;
  font-size: 13px;
  background: #fff;
  color: #333;
  cursor: pointer;
  transition: background 0.15s;
}

.btn:hover {
  background: #f2f4f7;
}

.btn.primary {
  background: #4f7cff;
  border-color: #4f7cff;
  color: #fff;
}

.btn.primary:hover {
  background: #3b67e8;
}

.btn.small {
  padding: 5px 12px;
  font-size: 13px;
}

.btn.danger:hover {
  background: #ffe3e3;
  color: #d33;
}

/* remixicon 图标与按钮文字对齐 */
.btn .ri {
  vertical-align: -2px;
  margin-right: 4px;
}

.btn.ctl .ri {
  margin-right: 0;
}

.warn-icon {
  vertical-align: -2px;
  margin-right: 5px;
  color: #c0392b;
}

.list {
  flex: 1;
  padding: 14px 18px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  min-height: 0;
}

.list-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 12px;
}

/* 视图分段控件：两枚按钮拼接，激活蓝底白字 */
.view-switch {
  display: inline-flex;
  border: 1px solid rgba(0, 0, 0, 0.1);
  border-radius: 8px;
  overflow: hidden;
  background: #fff;
}

/* 页面切换页签（便签 / 任务总览） */
.page-switch {
  margin-left: 12px;
  margin-right: auto;
}
.page-switch button .ri {
  margin-right: 3px;
  vertical-align: -1px;
}

/* 任务总览页容器 */
.todo-page {
  flex: 1;
  padding: 12px 18px;
  overflow-y: auto;
}

.view-switch button {
  border: none;
  background: transparent;
  padding: 6px 14px;
  font-size: 13px;
  color: #555;
  cursor: pointer;
  transition: background 0.15s, color 0.15s;
}

.view-switch button + button {
  border-left: 1px solid rgba(0, 0, 0, 0.08);
}

.view-switch button.on {
  background: #4f7cff;
  color: #fff;
}

.view-switch button:not(.on):hover {
  background: #f2f4f7;
}

.group-create {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.group-create-input {
  width: 140px;
  padding: 5px 10px;
  font-size: 13px;
  border: 1px solid rgba(0, 0, 0, 0.14);
  border-radius: 8px;
  outline: none;
}

.group-create-input:focus {
  border-color: #4f7cff;
}

/* —— 分区视图：左列（文件夹树+便签）/ 分隔线 / 右列预览 —— */
.section-split {
  position: relative;
  flex: 1;
  min-height: 0;
  display: flex;
  align-items: stretch;
}

.section-main {
  flex: 1;
  min-width: 200px;
  overflow-y: auto;
  padding-right: 6px;
}

.section-split.collapsed .gutter,
.section-split.collapsed .section-preview {
  display: none;
}

.gutter {
  flex: none;
  width: 8px;
  cursor: col-resize;
  position: relative;
  background: linear-gradient(to right, transparent 0 3px, rgba(0, 0, 0, 0.07) 3px 5px, transparent 5px);
}

.gutter:hover {
  background: linear-gradient(to right, transparent 0 2px, #4f7cff 2px 6px, transparent 6px);
}

.section-preview {
  flex: none;
  min-width: 0;
  border-left: 1px solid rgba(0, 0, 0, 0.06);
  background: rgba(255, 255, 255, 0.6);
  display: flex;
}

/* 收起后的「展开预览」手柄（贴在右缘） */
.preview-stub {
  position: absolute;
  right: 0;
  top: 50%;
  transform: translateY(-50%);
  writing-mode: vertical-rl;
  border: 1px solid rgba(0, 0, 0, 0.1);
  border-right: none;
  border-radius: 8px 0 0 8px;
  background: #fff;
  color: #666;
  font-size: 11.5px;
  padding: 10px 4px;
  cursor: pointer;
  box-shadow: -2px 0 8px rgba(0, 0, 0, 0.06);
  display: inline-flex;
  align-items: center;
  gap: 3px;
}

.preview-stub:hover {
  color: #4f7cff;
}

/* —— 分区视图（分组块） —— */
.group-block {
  margin-bottom: 10px;
}

.group-head {
  position: relative;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 7px 12px;
  background: #fbf7ec;
  border-radius: 8px;
  cursor: pointer;
  user-select: none;
  transition: background 0.15s;
  border: 1px solid transparent;
}

.group-head:hover {
  background: #f5efe0;
}

/* 树形子分组：左侧引导色条 */
.group-head.group-sub {
  background: #fdfbf4;
  border-left: 2px solid rgba(79, 124, 255, 0.28);
}

.group-head.dragging {
  opacity: 0.45;
}

/* 拖拽放置指示：上下蓝线 / 中间高亮（成为子分组） */
.group-head.drop-before::before,
.group-head.drop-after::after {
  content: "";
  position: absolute;
  left: 6px;
  right: 6px;
  height: 2px;
  border-radius: 2px;
  background: #4f7cff;
}

.group-head.drop-before::before {
  top: -1px;
}

.group-head.drop-after::after {
  bottom: -1px;
}

.group-head.drop-inside {
  background: rgba(79, 124, 255, 0.14);
  border-color: #4f7cff;
}

.grip {
  flex: none;
  color: #c9bfa6;
  font-size: 13px;
  cursor: grab;
  opacity: 0;
  transition: opacity 0.12s;
}

.group-head:hover .grip {
  opacity: 1;
}

/* 文件夹图标：默认蓝色（分组颜色只作用于分组头背景） */
.folder {
  flex: none;
  font-size: 15px;
  display: grid;
  place-items: center;
  color: #4f7cff;
}

.group-swatch {
  flex: none;
  width: 9px;
  height: 9px;
  border-radius: 3px;
  box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.08);
}

.child-create {
  display: flex;
  align-items: center;
  gap: 6px;
  margin: 6px 0;
}

.caret {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  font-size: 16px;
  line-height: 1;
  color: #998a66;
  width: 18px;
  flex: none;
}

.group-name {
  font-size: 14px;
  font-weight: 600;
  color: #444;
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.group-rename {
  flex: 1;
  min-width: 0;
  font-size: 14px;
  font-weight: 600;
  padding: 2px 8px;
  border: 1px solid #4f7cff;
  border-radius: 6px;
  outline: none;
}

.group-count {
  flex: none;
  min-width: 20px;
  text-align: center;
  font-size: 11px;
  line-height: 1;
  padding: 4px 8px;
  border-radius: 999px;
  background: rgba(79, 124, 255, 0.12);
  color: #3b67e8;
}

.group-menu-btn {
  flex: none;
  padding: 2px 9px;
  font-size: 14px;
  line-height: 1.4;
}

.dropdown {
  position: absolute;
  top: calc(100% + 4px);
  right: 8px;
  z-index: 30;
  display: flex;
  flex-direction: column;
  min-width: 140px;
  background: #fff;
  border: 1px solid rgba(0, 0, 0, 0.08);
  border-radius: 8px;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.14);
  padding: 4px;
  animation: dropdown-in 0.12s ease-out;
}

@keyframes dropdown-in {
  from {
    opacity: 0;
    transform: translateY(-3px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

.dropdown button {
  border: none;
  background: transparent;
  text-align: left;
  padding: 7px 10px;
  font-size: 13px;
  color: #333;
  border-radius: 6px;
  cursor: pointer;
  display: flex;
  align-items: center;
  gap: 7px;
}

.dropdown button:hover {
  background: #f2f4f7;
}

.dropdown button.danger-item {
  color: #d33;
}

.dropdown button.danger-item:hover {
  background: #ffe3e3;
}

.dropdown-sep {
  height: 1px;
  background: rgba(0, 0, 0, 0.07);
  margin: 4px 6px;
}

/* 「修改样式」调色板 */
.dropdown.palette {
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
  padding: 0;
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

.group-empty {
  margin: 8px 2px;
  font-size: 12px;
  color: #b9b2a2;
}

.cards {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-top: 8px;
}

.card-cell {
  display: block;
}

.empty {
  color: #999;
  font-size: 14px;
  text-align: center;
  margin-top: 48px;
}

/* —— 平铺视图：按文件夹分行 —— */
.filter-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-bottom: 12px;
}

.filter-chips button {
  border: 1px solid rgba(0, 0, 0, 0.1);
  border-radius: 999px;
  padding: 5px 13px;
  font-size: 12px;
  background: #fff;
  color: #555;
  cursor: pointer;
  transition: background 0.15s, color 0.15s;
}

.filter-chips button:hover {
  background: #f2f4f7;
}

.filter-chips button.on {
  background: #4f7cff;
  border-color: #4f7cff;
  color: #fff;
}

.flat-row {
  border-left: 3px solid #c9ccd3;
  padding-left: 12px;
  margin-bottom: 16px;
}

.flat-head {
  display: flex;
  align-items: center;
  gap: 7px;
  margin-bottom: 6px;
}

.flat-name {
  font-size: 13.5px;
  font-weight: 600;
  color: #444;
}

.flat-path {
  font-size: 11px;
  color: #b9b2a2;
  margin-left: 4px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* 弹窗 */
.confirm-mask {
  position: absolute;
  inset: 0;
  background: rgba(0, 0, 0, 0.3);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 40;
}

.confirm-box {
  width: 320px;
  background: #fff;
  border-radius: 12px;
  padding: 18px;
  box-shadow: 0 12px 40px rgba(0, 0, 0, 0.3);
}

.confirm-box h3 {
  margin: 0 0 10px;
  font-size: 15px;
  color: #333;
}

.confirm-box p {
  margin: 0 0 16px;
  font-size: 13px;
  color: #555;
  line-height: 1.6;
}

.choice {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 0 0 8px;
  font-size: 13px;
  color: #444;
  cursor: pointer;
}

.choice input {
  accent-color: #4f7cff;
  cursor: pointer;
}

.warn-line {
  margin-top: 4px !important;
  padding: 8px 10px;
  border-radius: 8px;
  background: #fff2f2;
  color: #c0392b !important;
  font-size: 12px !important;
}

.confirm-actions {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
}

.btn.danger {
  background: #e5484d;
  border-color: #e5484d;
  color: #fff;
}

.btn.danger:hover {
  background: #d33;
}

/* toast */
.group-toast {
  position: absolute;
  bottom: 24px;
  left: 50%;
  transform: translateX(-50%);
  background: rgba(34, 34, 34, 0.92);
  color: #fff;
  font-size: 13px;
  padding: 8px 16px;
  border-radius: 8px;
  box-shadow: 0 6px 20px rgba(0, 0, 0, 0.25);
  z-index: 50;
  animation: toast-in 0.15s ease-out;
}

@keyframes toast-in {
  from {
    opacity: 0;
    transform: translateX(-50%) translateY(6px);
  }
  to {
    opacity: 1;
    transform: translateX(-50%) translateY(0);
  }
}
</style>
