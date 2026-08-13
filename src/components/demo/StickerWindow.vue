<script setup lang="ts">
import { ref, computed, onMounted } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";

// 按 label 序号从调色板取默认背景色（每个窗口颜色不同）
const PALETTE = ["#FFF4D6", "#D6E9FF", "#E4F5D8", "#F8DFF0", "#E8E4FF", "#FFE9D2"];

const label = ref("");
const alphaPct = ref(85); // 15~100
const bgColor = ref("#FFF4D6");

const alpha = computed(() => (alphaPct.value / 100).toFixed(2));

function hexToRgba(hex: string, a: number): string {
  const m = /^#?([0-9a-fA-F]{6})$/.exec(hex.trim());
  if (!m) return `rgba(255, 244, 214, ${a})`;
  const n = parseInt(m[1], 16);
  const r = (n >> 16) & 0xff;
  const g = (n >> 8) & 0xff;
  const b = n & 0xff;
  return `rgba(${r}, ${g}, ${b}, ${a})`;
}

const cardStyle = computed(() => ({
  background: hexToRgba(bgColor.value, Number(alpha.value)),
}));

onMounted(async () => {
  label.value = getCurrentWindow().label;
  // sticker-<n> → 调色板取色（演示：颜色随窗口不同）
  const m = /sticker-(\d+)/.exec(label.value);
  if (m) {
    bgColor.value = PALETTE[(Number(m[1]) - 1) % PALETTE.length];
  }
});
</script>

<template>
  <div class="sticker" :style="cardStyle" data-tauri-drag-region>
    <div class="toolbar" data-tauri-drag-region>
      <span class="win-label">{{ label }}</span>
      <input v-model.number="alphaPct" type="range" min="15" max="100" class="alpha-slider" />
      <input v-model="bgColor" type="color" class="color-picker" title="背景颜色" />
      <span class="pct">{{ alphaPct }}%</span>
    </div>

    <div class="content">
      <h2>便签标题</h2>
      <p>这是便签正文。背景半透明、文字不透明——拖动上方滑块可独立调整本窗口背景透明度。</p>
    </div>
  </div>
</template>

<style scoped>
.sticker {
  height: 100vh;
  box-sizing: border-box;
  margin: 8px;
  border-radius: 14px;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  box-shadow: 0 6px 24px rgba(0, 0, 0, 0.2);
}

/* 工具条保持不透明，保证滑块/取色器可读可操作 */
.toolbar {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
  background: rgba(255, 255, 255, 0.92);
  border-bottom: 1px solid rgba(0, 0, 0, 0.08);
  font-size: 12px;
  color: #666;
  cursor: grab;
}

.win-label {
  font-family: Consolas, monospace;
  min-width: 76px;
}

.alpha-slider {
  flex: 1;
  accent-color: #4f7cff;
}

.color-picker {
  width: 26px;
  height: 22px;
  border: none;
  padding: 0;
  background: transparent;
  cursor: pointer;
}

.pct {
  min-width: 36px;
  text-align: right;
  font-variant-numeric: tabular-nums;
}

.content {
  padding: 16px;
  flex: 1;
}

.content h2 {
  margin: 0 0 8px;
  font-size: 17px;
  color: #333;
}

.content p {
  margin: 0;
  font-size: 13px;
  line-height: 1.7;
  color: #444;
}
</style>
