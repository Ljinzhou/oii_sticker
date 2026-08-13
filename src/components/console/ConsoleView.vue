<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount } from "vue";
import { useNotesStore } from "../../stores/notes";
import { useSettingsStore } from "../../stores/settings";
import { listen } from "../../composables/useTauri";
import type { UnlistenFn } from "@tauri-apps/api/event";
import type { NewSticker, Sticker } from "../../types";
import SettingsPanel from "./SettingsPanel.vue";

const notes = useNotesStore();
const settings = useSettingsStore();
const showSettings = ref(false);
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
  always_on_top: false,
  auto_scroll: false,
});

async function createSticker() {
  await notes.create(newSticker());
}

async function removeSticker(s: Sticker) {
  await notes.remove(s.id);
}

function preview(sticker: Sticker): string {
  const text = sticker.content.replace(/[#>*`\[\]]/g, "").trim();
  return text.length > 40 ? text.slice(0, 40) + "…" : text;
}

function reminderText(s: Sticker): string {
  // 提醒信息由 attrs 提供；列表简化显示模式即可
  return s.display_mode;
}

onMounted(async () => {
  notes.refresh();
  settings.refresh();
  // 后端推送 → 刷新列表
  unlisteners.push(await listen("sticky://push-update", () => notes.refresh()));
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
      </div>
    </header>

    <section class="list">
      <p v-if="notes.loading" class="empty">加载中…</p>
      <p v-else-if="notes.stickers.length === 0" class="empty">暂无便签，点击"新建便签"开始</p>
      <div v-else class="cards">
        <div v-for="s in notes.stickers" :key="s.id" class="card">
          <div class="card-head">
            <span class="card-title">{{ s.title || "（无标题）" }}</span>
            <span class="mode-badge">{{ reminderText(s) }}</span>
            <button class="btn small danger" title="删除便签（保留数据？否——直接删除）" @click="removeSticker(s)">✕</button>
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
  </main>
</template>

<style scoped>
.console {
  height: 100vh;
  box-sizing: border-box;
  margin: 8px;
  border-radius: 14px;
  background: rgba(255, 255, 255, 0.94);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.18);
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
  padding: 3px 8px;
  font-size: 12px;
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

.card-title {
  font-size: 14px;
  font-weight: 600;
  color: #222;
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.mode-badge {
  font-size: 11px;
  color: #4f7cff;
  background: rgba(79, 124, 255, 0.1);
  border-radius: 6px;
  padding: 2px 8px;
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
</style>
