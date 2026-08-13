<script setup lang="ts">
import { ref, onMounted } from "vue";
import { invoke } from "@tauri-apps/api/core";

const windows = ref<string[]>([]);
let seq = 0;

async function refresh() {
  windows.value = await invoke<string[]>("list_sticker_windows");
}

async function createWindow() {
  seq += 1;
  const offset = 80 + seq * 40;
  await invoke<string>("create_sticker_window", {
    title: `便签 ${seq}`,
    x: 300 + offset,
    y: 150 + offset,
  });
  await refresh();
}

onMounted(refresh);
</script>

<template>
  <main class="console">
    <header class="console-header" data-tauri-drag-region>
      <h1>oii_sticker 主控台</h1>
      <button class="new-btn" @click="createWindow">＋ 新建便签</button>
    </header>

    <section class="list">
      <p v-if="windows.length === 0" class="empty">暂无便签窗口</p>
      <ul v-else>
        <li v-for="label in windows" :key="label">{{ label }}</li>
      </ul>
    </section>
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

.new-btn {
  border: none;
  border-radius: 8px;
  padding: 8px 14px;
  font-size: 14px;
  color: #fff;
  background: #4f7cff;
  cursor: pointer;
  transition: background 0.15s;
}

.new-btn:hover {
  background: #3b67e8;
}

.list {
  flex: 1;
  padding: 12px 18px;
  overflow-y: auto;
}

.empty {
  color: #999;
  font-size: 14px;
  text-align: center;
  margin-top: 40px;
}

ul {
  list-style: none;
  margin: 0;
  padding: 0;
}

li {
  padding: 8px 12px;
  border-radius: 8px;
  background: rgba(0, 0, 0, 0.04);
  font-size: 14px;
  color: #444;
  margin-bottom: 6px;
  font-family: Consolas, monospace;
}
</style>
