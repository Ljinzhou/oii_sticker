<script setup lang="ts">
import { ref, onMounted } from "vue";
import { useSettingsStore } from "../../stores/settings";
import { useNotesStore } from "../../stores/notes";
import { invoke } from "../../composables/useTauri";

const emit = defineEmits<{ close: [] }>();
const settings = useSettingsStore();
const notes = useNotesStore();

type MenuKey = "general" | "defaults" | "about" | "debug";
const activeMenu = ref<MenuKey>("general");
const autoStart = ref(false);
const closeBehavior = ref("hide");
const debugMode = ref(true);

async function toggleDebugMode() {
  debugMode.value = !debugMode.value;
  await settings.set("debug_mode", debugMode.value ? "1" : "0");
  log(`调试模式：${debugMode.value ? "开启（详细日志）" : "关闭"}`);
}

async function setCloseBehavior(v: string) {
  closeBehavior.value = v;
  await settings.set("main_close_behavior", v);
  log(`关闭主控台行为：${v === "quit" ? "退出程序" : "隐藏到托盘"}`);
}

// Debug 状态
const notifyResult = ref("");
const dbInfo = ref<{ user_version: number; tables: string[]; db_path: string; config_keys: number } | null>(null);
const debugLog = ref<string[]>([]);

function log(msg: string) {
  debugLog.value.unshift(`[${new Date().toLocaleTimeString()}] ${msg}`);
}

async function toggleAutoStart() {
  if (autoStart.value) {
    await invoke("plugin:autostart|disable");
    autoStart.value = false;
  } else {
    await invoke("plugin:autostart|enable");
    autoStart.value = true;
  }
  log(`开机自启：${autoStart.value ? "已启用" : "已禁用"}`);
}

async function sendTestNotify() {
  notifyResult.value = "发送中…";
  try {
    await invoke("debug_notify_cmd", {
      title: "oii_sticker 测试通知",
      body: "这是一条来自 Debug 菜单的测试通知。",
    });
    notifyResult.value = "已发送 ✅（请查看系统通知）";
    log("测试通知已发送");
  } catch (e) {
    notifyResult.value = `发送失败：${e}`;
    log(`测试通知失败：${e}`);
  }
}

async function checkDbHealth() {
  try {
    const info = await invoke<{ user_version: number; tables: string[]; db_path: string; config_keys: number }>("db_health");
    dbInfo.value = info;
    log(`DB 健康检查：v${info.user_version}，${info.tables.length} 张表，${info.config_keys} 个配置键`);
  } catch (e) {
    log(`DB 健康检查失败：${e}`);
  }
}

async function createTestSticker() {
  try {
    await notes.create({
      title: "测试便签",
      content: "# 测试便签\n\n这是 Debug 菜单创建的测试便签。",
      pos_x: 250,
      pos_y: 180,
      width: 360,
      height: 420,
      opacity: settings.opacity,
      bg_color: "#D6E9FF",
    });
    log("测试便签已创建");
  } catch (e) {
    log(`创建测试便签失败：${e}`);
  }
}

onMounted(async () => {
  settings.refresh();
  try {
    autoStart.value = await invoke<boolean>("plugin:autostart|is_enabled");
  } catch {
    autoStart.value = false;
  }
  closeBehavior.value = settings.get("main_close_behavior", "hide");
  debugMode.value = settings.get("debug_mode", "1") === "1";
});
</script>

<template>
  <div class="settings-page">
    <!-- 顶部拖拽条：设置面板覆盖主控台后仍可拖动窗口 -->
    <div class="drag-bar" data-tauri-drag-region></div>
    <aside class="nav">
      <h2 class="page-title">系统设置</h2>
      <button
        class="nav-item"
        :class="{ active: activeMenu === 'general' }"
        @click="activeMenu = 'general'"
      >通用设置</button>
      <button
        class="nav-item"
        :class="{ active: activeMenu === 'defaults' }"
        @click="activeMenu = 'defaults'"
      >便签样式</button>
      <button
        class="nav-item"
        :class="{ active: activeMenu === 'debug' }"
        @click="activeMenu = 'debug'"
      >Debug</button>
      <button
        class="nav-item"
        :class="{ active: activeMenu === 'about' }"
        @click="activeMenu = 'about'"
      >关于</button>
    </aside>

    <section class="content">
      <!-- 通用 -->
      <div v-if="activeMenu === 'general'">
        <h3>通用</h3>
        <label class="row">
          <span>开机自启</span>
          <input type="checkbox" :checked="autoStart" @change="toggleAutoStart" />
        </label>
        <label class="row">
          <span>关闭主控台时</span>
          <select :value="closeBehavior" @change="(e) => setCloseBehavior((e.target as HTMLSelectElement).value)">
            <option value="hide">隐藏到系统托盘</option>
            <option value="quit">退出程序</option>
          </select>
        </label>
        <label class="row">
          <span>交互模式自动收起（秒）</span>
          <input
            type="number"
            min="1"
            max="60"
            :value="settings.get('auto_collapse_secs', '5')"
            @change="(e) => settings.set('auto_collapse_secs', (e.target as HTMLInputElement).value)"
          />
        </label>
        <p class="hint">便签进入交互模式后无操作满该秒数自动恢复展示模式（编辑/设置打开时不收起）。</p>
        <p class="hint">点击主控台右上角 ✕ 时的行为（隐藏后可从托盘图标恢复）。</p>
      </div>

      <!-- 便签默认 -->
      <div v-else-if="activeMenu === 'defaults'">
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
          <span>正文字号</span>
          <input
            type="number"
            min="9"
            max="28"
            :value="settings.bodyFontSize"
            @change="(e) => settings.set('default_sticker_body_font_size', (e.target as HTMLInputElement).value)"
          />
        </label>
        <label class="row">
          <span>编辑模式字号</span>
          <input
            type="number"
            min="10"
            max="36"
            :value="settings.get('edit_font_size', '14')"
            @change="(e) => settings.set('edit_font_size', (e.target as HTMLInputElement).value)"
          />
        </label>
        <label class="row">
          <span>编辑模式显示行号</span>
          <input
            type="checkbox"
            :checked="settings.get('editor_line_numbers', '0') === '1'"
            @change="(e) => settings.set('editor_line_numbers', (e.target as HTMLInputElement).checked ? '1' : '0')"
          />
        </label>
        <label class="row">
          <span>编辑模式默认形态</span>
          <select :value="settings.get('editor_mode', 'markdown')" @change="(e) => settings.set('editor_mode', (e.target as HTMLSelectElement).value)">
            <option value="markdown">Markdown（原生文本）</option>
            <option value="live">即时预览（渲染即编辑）</option>
          </select>
        </label>
        <p class="hint">编辑模式（WYSIWYG）下内容文字的字号，独立于便签正文字号。</p>
        <p class="hint">开启后在编辑模式左侧显示 Markdown 文本的行号。</p>
        <p class="hint">可在便签编辑模式左上角开关即时切换，此处为全局默认。</p>
      </div>

      <!-- Debug -->
      <div v-else-if="activeMenu === 'debug'">
        <h3>Debug 工具</h3>
        <label class="row">
          <span>调试模式（详细日志）</span>
          <input type="checkbox" :checked="debugMode" @change="toggleDebugMode" />
        </label>
        <p class="hint">开启后输出详细的操作/命令/事件日志（控制台）。</p>
        <div class="debug-actions">
          <button class="btn" @click="sendTestNotify">🔔 发送测试通知</button>
          <button class="btn" @click="checkDbHealth">🗄 数据库健康检查</button>
          <button class="btn" @click="createTestSticker">📝 创建测试便签</button>
        </div>
        <p v-if="notifyResult" class="result">{{ notifyResult }}</p>
        <pre v-if="dbInfo" class="db-info">
数据库版本：v{{ dbInfo.user_version }}
配置键数量：{{ dbInfo.config_keys }}
表：{{ dbInfo.tables.join(", ") }}
路径：{{ dbInfo.db_path }}
        </pre>
        <div v-if="debugLog.length" class="log">
          <div v-for="(l, i) in debugLog" :key="i" class="log-line">{{ l }}</div>
        </div>
      </div>

      <!-- 关于 -->
      <div v-else-if="activeMenu === 'about'">
        <h3>关于</h3>
        <p class="about-line"><strong>oii_sticker</strong> v0.1.0</p>
        <p class="about-line">Tauri 2 + Vue 3 桌面便签应用</p>
        <p class="about-line">重写自 <code>oi_sticker</code>（GPL v3）</p>
        <p class="about-line">数据库：SQLite（user_version=5，兼容旧库）</p>
      </div>
    </section>

    <footer class="foot">
      <button class="btn primary" @click="emit('close')">完成</button>
    </footer>
  </div>
</template>

<style scoped>
.settings-page {
  position: absolute;
  inset: 0;
  background: rgba(255, 255, 255, 0.97);
  display: grid;
  grid-template-columns: 180px 1fr;
  grid-template-rows: 1fr auto;
  z-index: 30;
}

/* 顶部拖拽区（覆盖主控台后仍可拖动窗口） */
.drag-bar {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: 22px;
  cursor: grab;
  z-index: 5;
  -webkit-app-region: drag;
}

.nav {
  grid-row: 1;
  border-right: 1px solid rgba(0, 0, 0, 0.08);
  padding: 16px 10px;
  display: flex;
  flex-direction: column;
  gap: 4px;
  background: rgba(248, 249, 251, 0.9);
}

.page-title {
  margin: 0 0 10px 8px;
  font-size: 15px;
  color: #333;
}

.nav-item {
  border: none;
  background: none;
  text-align: left;
  padding: 9px 12px;
  border-radius: 8px;
  font-size: 13px;
  color: #555;
  cursor: pointer;
}

.nav-item:hover {
  background: rgba(0, 0, 0, 0.05);
}

.nav-item.active {
  background: rgba(79, 124, 255, 0.12);
  color: #4f7cff;
  font-weight: 600;
}

.content {
  grid-row: 1;
  padding: 18px 22px;
  overflow-y: auto;
}

h3 {
  margin: 0 0 12px;
  font-size: 14px;
  color: #333;
}

.row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 0;
  font-size: 13px;
  color: #444;
  max-width: 420px;
}

.row input[type="range"] {
  width: 180px;
  accent-color: #4f7cff;
}

.row select {
  border: 1px solid rgba(0, 0, 0, 0.15);
  border-radius: 6px;
  padding: 4px 8px;
  font-size: 13px;
}

.row input[type="number"] {
  width: 64px;
  border: 1px solid rgba(0, 0, 0, 0.15);
  border-radius: 6px;
  padding: 4px 6px;
}

.hint {
  font-size: 12px;
  color: #999;
  margin: 6px 0 0;
}

.debug-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-bottom: 12px;
}

.btn {
  border: 1px solid rgba(0, 0, 0, 0.12);
  border-radius: 8px;
  padding: 7px 14px;
  font-size: 13px;
  background: #fff;
  color: #333;
  cursor: pointer;
}

.btn:hover {
  background: #f2f4f7;
}

.btn.primary {
  background: #4f7cff;
  border-color: #4f7cff;
  color: #fff;
}

.result {
  font-size: 12px;
  color: #2e7d32;
  margin: 4px 0;
}

.db-info {
  background: rgba(0, 0, 0, 0.04);
  border-radius: 8px;
  padding: 10px 12px;
  font-size: 12px;
  color: #555;
  margin: 8px 0;
  font-family: Consolas, monospace;
  white-space: pre-wrap;
}

.log {
  margin-top: 10px;
  border-top: 1px dashed rgba(0, 0, 0, 0.12);
  padding-top: 8px;
  max-height: 200px;
  overflow-y: auto;
}

.log-line {
  font-size: 12px;
  color: #777;
  font-family: Consolas, monospace;
  padding: 2px 0;
}

.about-line {
  font-size: 13px;
  color: #555;
  margin: 6px 0;
}

.foot {
  grid-row: 2;
  grid-column: 1 / -1;
  display: flex;
  justify-content: flex-end;
  padding: 10px 18px;
  border-top: 1px solid rgba(0, 0, 0, 0.08);
}
</style>
