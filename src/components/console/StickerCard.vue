<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import type { Sticker } from "../../types";
import { useNotesStore } from "../../stores/notes";
import { setTitleInContent } from "../../utils/sticker-title";

const props = defineProps<{ sticker: Sticker; isOpen: boolean }>();
const emit = defineEmits<{
  toggle: [s: Sticker]; // 显示/隐藏窗口
  remove: [s: Sticker]; // 打开删除确认弹窗
  resetWindow: [s: Sticker]; // 重置窗口大小与位置
}>();

const notes = useNotesStore();

function preview(sticker: Sticker): string {
  const text = sticker.content.replace(/[#>*`\[\]]/g, "").trim();
  return text.length > 40 ? text.slice(0, 40) + "…" : text;
}

// —— ⋯ 更多菜单（下拉：重命名 / 转移分组 / 移出分组 / 删除便签）——
const rootEl = ref<HTMLElement | null>(null);
const menuOpen = ref(false);
const showMoveSub = ref(false);
const renaming = ref(false);
const titleDraft = ref("");
const renameInput = ref<HTMLInputElement | null>(null);

/** 除当前分组外的其他分组（转移目标列表） */
const otherGroups = computed(() =>
  notes.groups.filter((g) => g.id !== props.sticker.group_id),
);

function closeMenu() {
  menuOpen.value = false;
  showMoveSub.value = false;
}

function toggleMenu() {
  menuOpen.value = !menuOpen.value;
  if (!menuOpen.value) showMoveSub.value = false;
}

function startRename() {
  renaming.value = true;
  titleDraft.value = props.sticker.title;
  closeMenu();
  void nextTick(() => renameInput.value?.focus());
}

async function commitRename() {
  if (!renaming.value) return; // Esc 取消后 blur 触发的兜底守卫
  const t = titleDraft.value.trim();
  if (t && t !== props.sticker.title) {
    // 标题 = 内容中第一个 # 一级标题 → 改名必须同步改写内容，否则下次保存会被内容覆盖
    await notes.update(props.sticker.id, {
      title: t,
      content: setTitleInContent(props.sticker.content, t),
    });
  }
  renaming.value = false;
}

function cancelRename() {
  renaming.value = false;
}

async function moveTo(gid: number) {
  closeMenu();
  await notes.moveStickerToGroup(props.sticker.id, gid);
}

async function moveOut() {
  closeMenu();
  await notes.moveStickerToGroup(props.sticker.id, null);
}

function requestRemove() {
  closeMenu();
  emit("remove", props.sticker);
}

async function requestResetWindow() {
  closeMenu();
  emit("resetWindow", props.sticker);
}

// document 级 pointerdown 关闭菜单（跨卡片互斥）：
// - 本卡 ⋯ 按钮：pointerdown 先于 click，忽略以免双切换（由 toggleMenu 自管开合）
// - 本卡下拉菜单内部：交给 @click.stop 的菜单项处理，不关闭
// - 本卡其他区域（如显示/隐藏按钮）：关闭
// - 其他卡片或页面空白：关闭 → 同一时刻至多一张卡菜单打开
function onDocPointerDown(e: PointerEvent) {
  const t = e.target as HTMLElement | null;
  if (rootEl.value && t && rootEl.value.contains(t)) {
    if (t.closest(".more") || t.closest(".card-dropdown")) return;
    closeMenu();
    return;
  }
  if (menuOpen.value) closeMenu();
}

function onDocKeydown(e: KeyboardEvent) {
  if (e.key === "Escape" && menuOpen.value) closeMenu();
}

onMounted(() => {
  document.addEventListener("pointerdown", onDocPointerDown);
  document.addEventListener("keydown", onDocKeydown);
});

onBeforeUnmount(() => {
  document.removeEventListener("pointerdown", onDocPointerDown);
  document.removeEventListener("keydown", onDocKeydown);
});
</script>

<template>
  <div ref="rootEl" class="card">
    <div class="card-head">
      <input
        v-if="renaming"
        ref="renameInput"
        v-model="titleDraft"
        class="card-title-edit"
        @click.stop
        @keydown.enter.prevent="commitRename"
        @keydown.esc="cancelRename"
        @blur="commitRename"
      />
      <span v-else class="card-title">{{ props.sticker.title || "（无标题）" }}</span>
      <div class="card-btns">
        <button class="btn small more" title="更多操作" @click.stop="toggleMenu"><i class="ri-more-2-fill"></i></button>
        <button
          class="btn small"
          :class="{ show: !props.isOpen }"
          :title="props.isOpen ? '隐藏窗口' : '显示窗口'"
          @click="emit('toggle', props.sticker)"
        >
          {{ props.isOpen ? "隐藏" : "显示" }}
        </button>
        <!-- 更多菜单 -->
        <div v-if="menuOpen" class="dropdown card-dropdown" @click.stop>
          <button @click="startRename">重命名</button>
          <button class="has-sub" @click.stop="showMoveSub = !showMoveSub">
            转移分组 <span class="sub-caret"><i class="ri-arrow-right-s-line"></i></span>
          </button>
          <div v-if="showMoveSub" class="submenu">
            <button v-for="g in otherGroups" :key="g.id" @click="moveTo(g.id)">
              {{ g.name }}
            </button>
            <p v-if="otherGroups.length === 0" class="submenu-empty">无其他分组</p>
          </div>
          <button :disabled="props.sticker.group_id == null" @click="moveOut">移出分组</button>
          <button @click="requestResetWindow">重置窗口大小与位置</button>
          <hr class="menu-sep" />
          <button class="danger-item" @click="requestRemove">删除便签</button>
        </div>
      </div>
    </div>
    <div class="card-preview">{{ preview(props.sticker) }}</div>
    <div class="card-foot">
      <span class="id">#{{ props.sticker.id }}</span>
    </div>
  </div>
</template>

<style scoped>
.card {
  border: 1px solid rgba(0, 0, 0, 0.08);
  border-radius: 10px;
  padding: 10px 14px;
  /* 与主控台背景透明度联动（同便签模式：背景透、文字不透明）；主控台外使用时不透明 */
  background: rgba(255, 255, 255, var(--console-alpha, 1));
  overflow: visible; /* 不裁剪下拉菜单 */
}

.card-head {
  display: flex;
  align-items: center;
  gap: 8px;
}

/* 右侧按钮组：放大 + 垂直居中 + 作为下拉定位锚 */
.card-btns {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: none;
  margin-left: auto;
  position: relative;
}

.card-title {
  font-size: 16px;
  font-weight: 600;
  color: #222;
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.card-title-edit {
  flex: 1;
  min-width: 0;
  font-size: 15px;
  font-weight: 600;
  padding: 2px 8px;
  border: 1px solid #4f7cff;
  border-radius: 6px;
  outline: none;
}

.card-preview {
  margin-top: 6px;
  font-size: 12px;
  color: #777;
  line-height: 1.5;
}

.card-foot {
  margin-top: 6px;
  display: flex;
  gap: 12px;
  font-size: 11px;
  color: #aaa;
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

/* 按钮区统一放大 */
.card-btns .btn.small {
  padding: 7px 16px;
  font-size: 14px;
  line-height: 1.2;
}

.btn.small.more {
  padding: 7px 10px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}

.btn.small.more .ri {
  font-size: 16px;
  color: #666;
}

.btn.small.more:hover .ri {
  color: #4f7cff;
}

/* 便签隐藏时：显示按钮蓝底（醒目提示可恢复） */
.btn.small.show {
  background: #4f7cff;
  border-color: #4f7cff;
  color: #fff;
}

.btn.small.show:hover {
  background: #3b67e8;
}

/* —— 卡片更多菜单 —— */
.card-dropdown {
  position: absolute;
  right: 0;
  top: calc(100% + 4px);
  min-width: 150px;
  background: #fff;
  border: 1px solid rgba(0, 0, 0, 0.1);
  border-radius: 10px;
  box-shadow: 0 10px 30px rgba(0, 0, 0, 0.18);
  padding: 4px;
  z-index: 30;
  animation: menu-in 120ms ease;
}

@keyframes menu-in {
  from {
    opacity: 0;
    transform: translateY(-3px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

.card-dropdown button {
  display: block;
  width: 100%;
  text-align: left;
  padding: 8px 12px;
  font-size: 13px;
  border: none;
  background: none;
  border-radius: 6px;
  cursor: pointer;
  color: #333;
}

.card-dropdown button:hover:not(:disabled) {
  background: #f2f4f7;
}

.card-dropdown button:disabled {
  color: #bbb;
  cursor: default;
}

.card-dropdown .has-sub {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.sub-caret {
  display: inline-flex;
  align-items: center;
  font-size: 14px;
  line-height: 1;
  color: #999;
}

.submenu {
  padding-left: 12px;
  max-height: 200px;
  overflow-y: auto;
}

.submenu-empty {
  margin: 2px 8px;
  font-size: 12px;
  color: #bbb;
}

.menu-sep {
  border: none;
  border-top: 1px solid rgba(0, 0, 0, 0.08);
  margin: 4px 6px;
}

.card-dropdown .danger-item {
  color: #d33;
}

.card-dropdown .danger-item:hover:not(:disabled) {
  background: #ffe3e3;
}
</style>
