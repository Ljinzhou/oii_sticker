<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useNotesStore } from "../../stores/notes";
import { useSettingsStore } from "../../stores/settings";
import { invoke, listen } from "../../composables/useTauri";
import type { UnlistenFn } from "@tauri-apps/api/event";
import type { NewSticker, Sticker } from "../../types";
import SettingsPanel from "./SettingsPanel.vue";
import StickerCard from "./StickerCard.vue";

const notes = useNotesStore();
const settings = useSettingsStore();
const showSettings = ref(false);
const openIds = ref<number[]>([]);
const confirming = ref<Sticker | null>(null);
const unlisteners: UnlistenFn[] = [];

const newSticker = (): NewSticker => ({
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
});

async function createSticker() {
  try {
    await notes.create(newSticker());
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

// —— 视图模式（持久化 system_config） ——
const viewMode = ref<"section" | "flat">(
  settings.get("console_group_view", "section") === "flat" ? "flat" : "section",
);
function setViewMode(m: "section" | "flat") {
  viewMode.value = m;
  void settings.set("console_group_view", m);
}

type Section = {
  key: string;
  name: string;
  isDefault: boolean;
  groupId: number | null;
  stickers: Sticker[];
};
const groupSections = computed<Section[]>(() => {
  const byId = new Map<number, Sticker[]>(notes.groups.map((g) => [g.id, []]));
  const def: Sticker[] = [];
  for (const s of notes.stickers) {
    if (s.group_id != null && byId.has(s.group_id)) byId.get(s.group_id)!.push(s);
    else def.push(s);
  }
  return [
    { key: "default", name: "默认分组", isDefault: true, groupId: null, stickers: def },
    ...notes.groups.map((g) => ({
      key: String(g.id),
      name: g.name,
      isDefault: false,
      groupId: g.id,
      stickers: byId.get(g.id) ?? [],
    })),
  ];
});

// 折叠状态（会话级，不持久化）
const collapsed = ref<Record<string, boolean>>({});
function toggleCollapse(key: string) {
  collapsed.value[key] = !collapsed.value[key];
}

// 平铺筛选
const filter = ref<"all" | "default" | number>("all");
const flatList = computed(() => {
  if (filter.value === "all") return notes.stickers;
  if (filter.value === "default") return notes.stickers.filter((s) => s.group_id == null);
  return notes.stickers.filter((s) => s.group_id === filter.value);
});

// 分组操作
const creatingGroup = ref(false);
const newGroupName = ref("");
async function onCreateGroup() {
  const name = newGroupName.value.trim();
  if (!name) return;
  await notes.createGroup(name);
  await notes.refresh(); // createGroup 不回读，手动刷新使新分组立即可见
  newGroupName.value = "";
  creatingGroup.value = false;
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

// 删除分组三选确认框
const deletingGroup = ref<{ id: number; name: string; count: number } | null>(null);
const deleteChoice = ref<"to-default" | "with-stickers">("to-default");
const confirmingWithStickers = ref(false);
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
  else showGroupToast("分组已删除，便签已移回默认分组");
  deletingGroup.value = null;
  confirmingWithStickers.value = false;
}

// 组菜单（标题条 ⋯）
const groupMenuFor = ref<string | null>(null);

// 简易 toast（复用 WorkspaceManager 模式）
const groupToast = ref<string | null>(null);
let toastTimer: ReturnType<typeof setTimeout> | null = null;
function showGroupToast(text: string) {
  if (toastTimer) clearTimeout(toastTimer);
  groupToast.value = text;
  toastTimer = setTimeout(() => (groupToast.value = null), 3000);
}

onMounted(async () => {
  notes.refresh();
  settings.refresh();
  refreshOpenIds();
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
});
</script>

<template>
  <main class="console">
    <header class="console-header" data-tauri-drag-region>
      <h1>oii_sticker 主控台</h1>
      <div class="actions">
        <button class="btn primary" @click="createSticker">＋ 新建便签</button>
        <button class="btn" @click="notes.refresh()">刷新</button>
        <button class="btn" @click="showSettings = true">系统设置</button>
        <span class="win-ctl">
          <button class="btn ctl" title="最小化" @click="minimizeWindow">─</button>
          <button class="btn ctl close" title="关闭" @click="closeWindow">✕</button>
        </span>
      </div>
    </header>

    <section class="list">
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
          <button v-else class="btn small" @click="creatingGroup = true">＋ 新建分组</button>
        </div>
      </div>

      <!-- 分区视图 -->
      <template v-if="viewMode === 'section'">
        <div v-for="sec in groupSections" :key="sec.key" class="group-block">
          <header class="group-head" @click="toggleCollapse(sec.key)">
            <span class="caret">{{ collapsed[sec.key] ? "▸" : "▾" }}</span>
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
            <span class="group-count">{{ sec.stickers.length }}</span>
            <button
              v-if="!sec.isDefault"
              class="btn small group-menu-btn"
              title="分组操作"
              @click.stop="groupMenuFor = groupMenuFor === sec.key ? null : sec.key"
            >
              ⋯
            </button>
            <div v-if="!sec.isDefault && groupMenuFor === sec.key" class="dropdown" @click.stop>
              <button @click="startRenameGroup({ id: sec.groupId!, name: sec.name }); groupMenuFor = null">
                重命名
              </button>
              <button
                class="danger-item"
                @click="
                  deletingGroup = { id: sec.groupId!, name: sec.name, count: sec.stickers.length };
                  deleteChoice = 'to-default';
                  confirmingWithStickers = false;
                  groupMenuFor = null;
                "
              >
                删除分组
              </button>
            </div>
          </header>
          <div v-show="!collapsed[sec.key]" class="cards">
            <p v-if="sec.stickers.length === 0" class="group-empty">
              {{ sec.isDefault ? "暂无便签" : "此分组暂无便签" }}
            </p>
            <StickerCard
              v-for="s in sec.stickers"
              :key="s.id"
              :sticker="s"
              :is-open="isOpen(s.id)"
              @toggle="toggleSticker"
              @remove="confirming = $event"
            />
          </div>
        </div>
      </template>

      <!-- 平铺视图 -->
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
            {{ sec.name }} {{ sec.stickers.length }}
          </button>
        </div>
        <div class="cards">
          <p v-if="flatList.length === 0" class="empty">
            {{ filter === "all" ? '暂无便签，点击"新建便签"开始' : "没有符合条件的便签" }}
          </p>
          <StickerCard
            v-for="s in flatList"
            :key="s.id"
            :sticker="s"
            :is-open="isOpen(s.id)"
            @toggle="toggleSticker"
            @remove="confirming = $event"
          />
        </div>
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
          <span>移回默认分组（便签保留）</span>
        </label>
        <label class="choice">
          <input v-model="deleteChoice" type="radio" value="with-stickers" />
          <span>连同便签一起删除</span>
        </label>
        <p v-if="confirmingWithStickers && deleteChoice === 'with-stickers'" class="warn-line">
          ⚠ 将永久删除这 {{ deletingGroup.count }} 张便签，不可恢复。再次点击「确认」执行。
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

.list {
  flex: 1;
  padding: 14px 18px;
  overflow-y: auto;
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

/* —— 分区视图 —— */
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
}

.group-head:hover {
  background: #f5efe0;
}

.caret {
  font-size: 11px;
  color: #998a66;
  width: 12px;
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
  min-width: 120px;
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

.empty {
  color: #999;
  font-size: 14px;
  text-align: center;
  margin-top: 48px;
}

/* —— 平铺视图 —— */
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
