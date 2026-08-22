<script setup lang="ts">
import type { Sticker } from "../../types";

const props = defineProps<{ sticker: Sticker; isOpen: boolean }>();
const emit = defineEmits<{
  toggle: [s: Sticker]; // 显示/隐藏窗口
  remove: [s: Sticker]; // 打开删除确认弹窗
}>();

function preview(sticker: Sticker): string {
  const text = sticker.content.replace(/[#>*`\[\]]/g, "").trim();
  return text.length > 40 ? text.slice(0, 40) + "…" : text;
}
</script>

<template>
  <div class="card">
    <div class="card-head">
      <span class="card-title">{{ props.sticker.title || "（无标题）" }}</span>
      <div class="card-btns">
        <button class="btn small danger del" title="删除便签" @click="emit('remove', props.sticker)">✕</button>
        <button
          class="btn small"
          :class="{ show: !props.isOpen }"
          :title="props.isOpen ? '隐藏窗口' : '显示窗口'"
          @click="emit('toggle', props.sticker)"
        >
          {{ props.isOpen ? "隐藏" : "显示" }}
        </button>
      </div>
    </div>
    <div class="card-preview">{{ preview(props.sticker) }}</div>
    <div class="card-foot">
      <span class="id">#{{ props.sticker.id }}</span>
      <span class="size">{{ props.sticker.width }}×{{ props.sticker.height }}</span>
    </div>
  </div>
</template>

<style scoped>
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
</style>
