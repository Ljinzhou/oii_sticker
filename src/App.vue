<script setup lang="ts">
// Phase 0 占位主控台。
// 验证 Demo 阶段将替换为按窗口 label 分发（main → Console / sticker-* → StickerWindow）。
</script>

<template>
  <main class="console">
    <h1>oii_sticker</h1>
    <p>阶段 0 脚手架占位界面（验证 Demo 阶段实现多窗口）</p>
  </main>
</template>

<style scoped>
.console {
  height: 100vh;
  box-sizing: border-box;
  margin: 8px;
  border-radius: 14px;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  color: #333;
  background: rgba(255, 255, 255, 0.92);
  user-select: none;
}
</style>
