<script setup lang="ts">
import { ref, onMounted } from "vue";
import { useSettingsStore } from "../../stores/settings";
import { invoke } from "../../composables/useTauri";

const emit = defineEmits<{ close: [] }>();
const settings = useSettingsStore();

const autoStart = ref(false);

async function toggleAutoStart() {
  if (autoStart.value) {
    await invoke("plugin:autostart|disable");
    autoStart.value = false;
  } else {
    await invoke("plugin:autostart|enable");
    autoStart.value = true;
  }
}

onMounted(async () => {
  try {
    autoStart.value = await invoke<boolean>("plugin:autostart|is_enabled");
  } catch {
    autoStart.value = false;
  }
});
</script>

<template>
  <div class="modal-mask" @click.self="emit('close')">
    <div class="panel">
      <header>
        <h2>系统设置</h2>
        <button class="close" @click="emit('close')">✕</button>
      </header>

      <section class="group">
        <h3>通用</h3>
        <label class="row">
          <span>开机自启</span>
          <input type="checkbox" :checked="autoStart" @change="toggleAutoStart" />
        </label>
      </section>

      <section class="group">
        <h3>新建便签默认偏好</h3>
        <label class="row">
          <span>背景透明度</span>
          <input
            type="range"
            min="15"
            max="100"
            :value="Math.round(settings.opacity * 100)"
            @change="(e) => settings.set('default_sticker_opacity', String(Number((e.target as HTMLInputElement).value) / 100))"
          />
        </label>
        <label class="row">
          <span>背景颜色</span>
          <input type="color" :value="settings.bgColor" @change="(e) => settings.set('default_sticker_bg_color', (e.target as HTMLInputElement).value)" />
        </label>
        <label class="row">
          <span>标题字号</span>
          <input
            type="number"
            min="10"
            max="32"
            :value="settings.titleFontSize"
            @change="(e) => settings.set('default_sticker_title_font_size', (e.target as HTMLInputElement).value)"
          />
        </label>
        <label class="row">
          <span>正文字号</span>
          <input
            type="number"
            min="9"
            max="28"
            :value="settings.bodyFontSize"
            @change="(e) => settings.set('default_sticker_body_font_size', (e.target as HTMLInputElement).value)"
          />
        </label>
      </section>

      <footer>
        <button class="btn primary" @click="emit('close')">完成</button>
      </footer>
    </div>
  </div>
</template>

<style scoped>
.modal-mask {
  position: absolute;
  inset: 0;
  background: rgba(0, 0, 0, 0.25);
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 14px;
}

.panel {
  width: 340px;
  max-height: 80%;
  overflow-y: auto;
  background: #fff;
  border-radius: 12px;
  padding: 16px;
  box-shadow: 0 12px 40px rgba(0, 0, 0, 0.3);
}

header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 8px;
}

h2 {
  margin: 0;
  font-size: 16px;
  color: #333;
}

.close {
  border: none;
  background: none;
  font-size: 14px;
  color: #999;
  cursor: pointer;
}

.group {
  margin-top: 12px;
}

h3 {
  margin: 0 0 8px;
  font-size: 12px;
  color: #888;
  font-weight: 600;
}

.row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 0;
  font-size: 13px;
  color: #444;
}

.row input[type="range"] {
  width: 140px;
  accent-color: #4f7cff;
}

.row input[type="number"] {
  width: 60px;
  border: 1px solid rgba(0, 0, 0, 0.15);
  border-radius: 6px;
  padding: 4px 6px;
}

footer {
  margin-top: 16px;
  text-align: right;
}

.btn {
  border: none;
  border-radius: 8px;
  padding: 8px 16px;
  font-size: 13px;
  cursor: pointer;
}

.btn.primary {
  background: #4f7cff;
  color: #fff;
}
</style>
