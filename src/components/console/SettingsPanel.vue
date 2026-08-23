<script setup lang="ts">
import { ref, onMounted } from "vue";
import { useSettingsStore } from "../../stores/settings";
import { invoke } from "../../composables/useTauri";
import WorkspaceManager from "./WorkspaceManager.vue";

const emit = defineEmits<{ close: [] }>();
const settings = useSettingsStore();

type MenuKey = "general" | "defaults" | "todo" | "workspace" | "about";
const activeMenu = ref<MenuKey>("general");
const autoStart = ref(false);
const autostartBusy = ref(false);
const autostartError = ref("");
const closeBehavior = ref("hide");

type AutostartInfo = {
  enabled: boolean;
  platform: string;
  mechanism: string;
  launch_args: string[];
  executable: string;
};

function applyAutostartInfo(info: AutostartInfo) {
  autoStart.value = info.enabled;
}

async function refreshAutostart() {
  autostartError.value = "";
  try {
    const info = await invoke<AutostartInfo>("autostart_get_cmd");
    applyAutostartInfo(info);
  } catch (e) {
    autostartError.value = "读取开机自启状态失败：" + String(e);
  }
}

async function toggleAutoStart() {
  const target = !autoStart.value;
  autostartBusy.value = true;
  autostartError.value = "";
  try {
    const info = await invoke<AutostartInfo>("autostart_set_cmd", { enabled: target });
    applyAutostartInfo(info);
    if (info.enabled !== target) {
      autostartError.value = "系统状态未按预期切换，请重试";
    }
  } catch (e) {
    autoStart.value = !target;
    autostartError.value = (target ? "开启" : "关闭") + "开机自启失败：" + String(e);
  } finally {
    autostartBusy.value = false;
  }
}

async function setCloseBehavior(v: string) {
  closeBehavior.value = v;
  await settings.set("main_close_behavior", v);
}

// 关于：更新检测 / 外链
const updateResult = ref<string>("");
const updateError = ref(false);
const checkingUpdate = ref(false);

async function checkUpdate() {
  checkingUpdate.value = true;
  updateResult.value = "检查中…";
  updateError.value = false;
  try {
    const info = await invoke<{ latest: string | null; current: string; has_update: boolean; error: string | null }>("check_update_cmd");
    if (info.error) {
      updateResult.value = info.error;
      updateError.value = true;
    } else if (info.has_update) {
      updateResult.value = `发现新版本 ${info.latest}（当前 ${info.current}），可前往仓库下载。`;
    } else {
      updateResult.value = `已是最新版本（${info.current}）。`;
    }
  } catch (e) {
    updateResult.value = `检查失败：${e}`;
    updateError.value = true;
  } finally {
    checkingUpdate.value = false;
  }
}

function openLink(url: string) {
  invoke("open_external_cmd", { url }).catch((e) => {
    updateResult.value = `打开链接失败：${e}`;
    updateError.value = true;
  });
}

onMounted(async () => {
  settings.refresh();
  await refreshAutostart();
  closeBehavior.value = settings.get("main_close_behavior", "hide");
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
      <button class="nav-item" :class="{ active: activeMenu === 'todo' }" @click="activeMenu = 'todo'">Todo 设置</button>
      <button
        class="nav-item"
        :class="{ active: activeMenu === 'workspace' }"
        @click="activeMenu = 'workspace'"
      >工作空间</button>
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
          <input type="checkbox" :checked="autoStart" :disabled="autostartBusy" @change="toggleAutoStart" />
        </label>
        <p v-if="autostartError" class="hint error">{{ autostartError }}</p>
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
        <p class="hint">点击主控台右上角「关闭」按钮时的行为（隐藏后可从托盘图标恢复）。</p>
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
          <span>编辑模式字体</span>
          <input
            type="text"
            :value="settings.get('edit_font_family', 'Microsoft YaHei')"
            @change="(e) => settings.set('edit_font_family', (e.target as HTMLInputElement).value.trim() || 'Microsoft YaHei')"
          />
        </label>
        <label class="row">
          <span>编辑模式显示行号（及时预览 / Markdown）</span>
          <input
            type="checkbox"
            :checked="settings.get('editor_line_numbers', '1') === '1'"
            @change="(e) => settings.set('editor_line_numbers', (e.target as HTMLInputElement).checked ? '1' : '0')"
          />
        </label>
        <label class="row">
          <span>编辑模式默认形态</span>
          <select :value="settings.get('editor_mode', 'markdown')" @change="(e) => settings.set('editor_mode', (e.target as HTMLSelectElement).value)">
            <option value="markdown">Markdown（原生文本）</option>
            <option value="live">及时预览（渲染即编辑）</option>
          </select>
        </label>
      </div>

      <div v-else-if="activeMenu === 'todo'">
        <h3>Todo 时间预设</h3>
        <label class="row">
          <span>Todo 窗口默认置顶</span>
          <input
            type="checkbox"
            :checked="settings.get('default_todo_always_on_top', '1') === '1'"
            @change="(e) => settings.set('default_todo_always_on_top', (e.target as HTMLInputElement).checked ? '1' : '0')"
          />
        </label>
        <label class="row"><span>明天提醒时间</span><input type="number" min="0" max="23" :value="settings.get('todo_remind_tomorrow_hour', '9')" @change="(e) => settings.set('todo_remind_tomorrow_hour', (e.target as HTMLInputElement).value)" /></label>
        <label class="row"><span>下周提醒星期</span><select :value="settings.get('todo_remind_next_week_dow', '1')" @change="(e) => settings.set('todo_remind_next_week_dow', (e.target as HTMLSelectElement).value)"><option v-for="(name, value) in ['周日','周一','周二','周三','周四','周五','周六']" :key="value" :value="value">{{ name }}</option></select></label>
        <label class="row"><span>下周提醒时间</span><input type="number" min="0" max="23" :value="settings.get('todo_remind_next_week_hour', '9')" @change="(e) => settings.set('todo_remind_next_week_hour', (e.target as HTMLInputElement).value)" /></label>
        <label class="row"><span>今天截止时间</span><input type="number" min="0" max="23" :value="settings.get('todo_due_today_hour', '18')" @change="(e) => settings.set('todo_due_today_hour', (e.target as HTMLInputElement).value)" /></label>
        <label class="row"><span>明天截止时间</span><input type="number" min="0" max="23" :value="settings.get('todo_due_tomorrow_hour', '9')" @change="(e) => settings.set('todo_due_tomorrow_hour', (e.target as HTMLInputElement).value)" /></label>
        <label class="row"><span>下周截止星期</span><select :value="settings.get('todo_due_next_week_dow', '1')" @change="(e) => settings.set('todo_due_next_week_dow', (e.target as HTMLSelectElement).value)"><option v-for="(name, value) in ['周日','周一','周二','周三','周四','周五','周六']" :key="value" :value="value">{{ name }}</option></select></label>
        <p class="hint">提醒和截止 chip 选择“明天”或“下周”时使用以上默认值。</p>
      </div>

      <!-- 工作空间 -->
      <div v-else-if="activeMenu === 'workspace'">
        <WorkspaceManager />
      </div>

      <!-- 关于 -->
      <div v-else-if="activeMenu === 'about'">
        <h3>关于</h3>

        <h4 class="about-sec">基本信息</h4>
        <p class="about-line"><strong>oii_sticker</strong> &nbsp;<span class="badge">v0.1.0</span></p>

        <h4 class="about-sec">程序介绍</h4>
        <p class="about-line">oii_sticker 是一款跨平台桌面便签应用，旨在提供轻量、高效的信息记录体验。聚焦以下能力：</p>
        <p class="about-line">• 便签：支持窗口拖拽 / 缩放 / 透明度与背景色调整，贴边收纳、托盘中隐藏与恢复。</p>
        <p class="about-line">• 富文本编辑：Markdown 源码模式与即时预览模式可自由切换，支持代码高亮与数学公式渲染。</p>
        <p class="about-line">• Todo 任务清单：独立窗口管理待办事项，配合日期预设（今天 / 明天 / 下周）与系统通知提醒。</p>
        <p class="about-line">• 分组工作空间：将便签按主题分组管理，便于整理与归档。</p>

        <h4 class="about-sec">更新检查</h4>
        <div class="about-actions">
          <button class="btn" :disabled="checkingUpdate" @click="checkUpdate">
            <i class="ri-refresh-line"></i>{{ checkingUpdate ? "检查中…" : "检查更新" }}
          </button>
          <button class="btn" @click="openLink('https://github.com/Ljinzhou/oii_sticker/releases')">
            <i class="ri-download-2-line"></i>查看仓库 / 下载
          </button>
        </div>
        <p v-if="updateResult" class="result" :class="{ error: updateError }">{{ updateResult }}</p>

        <h4 class="about-sec">开源许可</h4>
        <p class="about-line">本软件遵循 GPL v3 许可分发。</p>

        <h4 class="about-sec">作者</h4>
        <p class="about-line">李jinzhou（Ljinzhou）</p>
        <p class="about-line">当前Tokens充足，身份为“天才程序员”！</p>
        <p class="about-line"><button class="link-btn" @click="openLink('https://github.com/Ljinzhou')"><i class="ri-github-fill"></i>GitHub</button></p>
        <p class="about-line">邮箱：<span class="copy-mail">771625807@qq.com</span></p>

        <h4 class="about-sec">致谢</h4>
        <p class="about-line">感谢DeepSeek的惊人智慧，贡献了本项目99.99%的代码</p>
        <img class="about-credit" src="/thanks-deepseek.png" alt="感谢蓝色大肥鱼" />
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

.row input[type="text"] {
  width: 180px;
  border: 1px solid rgba(0, 0, 0, 0.15);
  border-radius: 6px;
  padding: 4px 6px;
  font-size: 13px;
}

.hint {
  font-size: 12px;
  color: #999;
  margin: 6px 0 0;
}

.hint.error {
  color: #c0392b;
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

.about-line {
  font-size: 13px;
  color: #555;
  margin: 6px 0;
}

.about-sec {
  margin: 16px 0 8px;
  font-size: 13px;
  color: #888;
  font-weight: 600;
  text-transform: none;
}

.about-sec:first-of-type {
  margin-top: 4px;
}

.badge {
  display: inline-block;
  background: rgba(79, 124, 255, 0.12);
  color: #4f7cff;
  border-radius: 10px;
  padding: 1px 8px;
  font-size: 12px;
  vertical-align: 1px;
}

.about-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin: 4px 0;
}

.about-actions .btn:disabled {
  opacity: 0.6;
  cursor: default;
}

.link-btn {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  border: 1px solid rgba(79, 124, 255, 0.35);
  background: rgba(79, 124, 255, 0.06);
  color: #4f7cff;
  border-radius: 8px;
  padding: 5px 12px;
  font-size: 13px;
  margin-right: 8px;
  cursor: pointer;
  text-decoration: none;
}

.link-btn:hover {
  background: rgba(79, 124, 255, 0.12);
}

.copy-mail {
  font-family: Consolas, monospace;
  color: #333;
  user-select: text;
  cursor: text;
}

.about-credit {
  margin-top: 10px;
  max-width: 260px;
  width: 100%;
  border-radius: 8px;
  border: 1px solid rgba(0, 0, 0, 0.08);
}

.result {
  font-size: 12px;
  color: #2e7d32;
  margin: 6px 0;
}

.result.error {
  color: #c0392b;
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