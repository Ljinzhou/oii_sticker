<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useNotesStore } from "../../stores/notes";
import { useSettingsStore } from "../../stores/settings";
import { invoke, listen } from "../../composables/useTauri";
import type { UnlistenFn } from "@tauri-apps/api/event";
import type { NewSticker, Sticker } from "../../types";
import SettingsPanel from "./SettingsPanel.vue";

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

function preview(sticker: Sticker): string {
  const text = sticker.content.replace(/[#>*`\[\]]/g, "").trim();
  return text.length > 40 ? text.slice(0, 40) + "…" : text;
}

function minimizeWindow() {
  getCurrentWindow().minimize();
}

function closeWindow() {
  // 按设置行为关闭：隐藏到托盘或退出程序（Rust 侧处理，无前端权限问题）
  invoke("main_close_cmd");
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
      <p v-if="notes.stickers.length === 0" class="empty">暂无便签，点击"新建便签"开始</p>
      <div v-else class="cards">
        <div v-for="s in notes.stickers" :key="s.id" class="card">
          <div class="card-head">
            <span class="card-title">{{ s.title || "（无标题）" }}</span>
            <div class="card-btns">
              <button class="btn small danger del" title="删除便签" @click="confirming = s">✕</button>
              <button
                class="btn small"
                :class="{ show: !isOpen(s.id) }"
                :title="isOpen(s.id) ? '隐藏窗口' : '显示窗口'"
                @click="toggleSticker(s)"
              >
                {{ isOpen(s.id) ? "隐藏" : "显示" }}
              </button>
            </div>
          </div>
          <div class="card-preview">{{ preview(s) }}</div>
          <div class="card-foot">
            <span class="id">#{{ s.id }}</span>
            <span class="size">{{ s.width }}×{{ s.height }}</span>
          </div>
        </div>
      </div>
    </section>

    <SettingsPanel v-if="showSettings" @close="showSettings = false" />

    <!-- 删除二次确认 -->
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

/* 便签隐藏时：显示按钮蓝底（醒目提示可恢复） */
.btn.small.show {
  background: #4f7cff;
  border-color: #4f7cff;
  color: #fff;
}

.btn.small.show:hover {
  background: #3b67e8;
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

.empty {
  color: #999;
  font-size: 14px;
  text-align: center;
  margin-top: 48px;
}

.cards {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.card {
  border: 1px solid rgba(0, 0, 0, 0.08);
  border-radius: 10px;
  padding: 10px 14px;
  background: #fff;
}

.card-head {
  display: flex;
  align-items: center;
  gap: 8px;
}

/* 右侧按钮组：删除（✕）紧挨显示按钮左侧 */
.card-btns {
  display: flex;
  gap: 6px;
  flex: none;
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

/* 删除确认弹窗 */
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
  width: 300px;
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
</style>
