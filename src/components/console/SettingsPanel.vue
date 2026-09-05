<script setup lang="ts">
import { computed, nextTick, ref, onMounted, onUnmounted } from "vue";
import { useSettingsStore } from "../../stores/settings";
import { invoke, listen } from "../../composables/useTauri";
import { downloadPercent, updateBarWidth, updateStageText } from "../../utils/update";
import WorkspaceManager from "./WorkspaceManager.vue";
import TodoPresetsManager from "./TodoPresetsManager.vue";

const emit = defineEmits<{ close: [] }>();
const settings = useSettingsStore();

type MenuKey = "general" | "defaults" | "todo" | "workspace" | "about";
const activeMenu = ref<MenuKey>("general");

// 版本号：由 vite define 注入（单一来源 package.json，构建期替换为实际版本）
const appVersion = __APP_VERSION__;
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

// 关于：自动更新（后端编排 + 事件驱动，见 src-tauri/src/updater.rs）
type UpdatePhase =
  | { phase: "idle" }
  | { phase: "checking"; current?: string }
  | { phase: "up_to_date"; current: string }
  | { phase: "available"; current: string; version: string; notes?: string | null }
  | { phase: "downloading"; downloaded: number; total: number | null; retrying: boolean }
  | { phase: "installing" }
  | { phase: "restarting" }
  | { phase: "failed"; code: string; message: string; manualUrl: string };

type UpdateError = { kind: string; message: string; manualUrl: string };

const upPhase = ref<string>("idle");
const upVersion = ref("");
const upProgressPct = ref<number | null>(null); // null = 总大小未知
const upRetrying = ref(false);
const upDownloaded = ref<number | null>(null);
const upTotal = ref<number | null>(null);
// 滚动日志框（方向 A：贴合现状的更新日志 + 整体进度条）
// 编辑模式字体下拉候选（Windows 常用中英文字体 + 通用族）
const FONT_OPTIONS: { value: string; label: string }[] = [
  { value: "Microsoft YaHei", label: "微软雅黑（默认）" },
  { value: "SimSun", label: "宋体" },
  { value: "SimHei", label: "黑体" },
  { value: "KaiTi", label: "楷体" },
  { value: "FangSong", label: "仿宋" },
  { value: "Consolas", label: "Consolas（等宽）" },
  { value: "Courier New", label: "Courier New" },
  { value: "system-ui", label: "系统默认（system-ui）" },
  { value: "monospace", label: "通用等宽（monospace）" },
];

const upLogs = ref<{ level: string; text: string }[]>([]);
const upLogEl = ref<HTMLElement | null>(null);
const upBarWidth = computed(() => updateBarWidth(upPhase.value, upDownloaded.value, upTotal.value));
const updateStage = computed(() => updateStageText(upPhase.value, upProgressPct.value, upRetrying.value));

async function pushLog(level: string, text: string) {
  upLogs.value.push({ level, text });
  if (upLogs.value.length > 200) upLogs.value.splice(0, upLogs.value.length - 200);
  await nextTick();
  if (upLogEl.value) upLogEl.value.scrollTop = upLogEl.value.scrollHeight;
}
const updateResult = ref<string>("");
const updateError = ref(false);
const checkingUpdate = ref(false);

function applyPhase(p: UpdatePhase) {
  upPhase.value = p.phase;
  upRetrying.value = false;
  switch (p.phase) {
    case "checking":
      break;
    case "up_to_date":
      updateResult.value = `已是最新版本（${p.current}）。`;
      updateError.value = false;
      break;
    case "available":
      upVersion.value = p.version;
      updateResult.value = "";
      break;
    case "downloading":
      upDownloaded.value = p.downloaded;
      upTotal.value = p.total ?? null;
      upProgressPct.value = downloadPercent(p.downloaded, p.total ?? null);
      upRetrying.value = p.retrying;
      break;
    case "installing":
      updateResult.value = "下载完成，正在安装…";
      updateError.value = false;
      break;
    case "restarting":
      updateResult.value = "安装完成，应用即将重启…";
      updateError.value = false;
      break;
    case "failed":
      updateResult.value = p.message;
      updateError.value = true;
      void pushLog("err", p.message);
      break;
    default:
      break;
  }
}

async function checkUpdate() {
  if (["downloading", "installing", "restarting", "checking"].includes(upPhase.value)) return;
  checkingUpdate.value = true;
  updateResult.value = "";
  updateError.value = false;
  try {
    const res = await invoke<{ phase: UpdatePhase }>("update_check_cmd");
    applyPhase(res.phase);
  } catch (e) {
    // 后端返回结构化错误 {kind,message,manualUrl}
    const err = e as Partial<UpdateError>;
    updateResult.value = err?.message ?? `检查失败：${String(e)}`;
    updateError.value = true;
  } finally {
    checkingUpdate.value = false;
  }
}

function startUpgrade() {
  updateError.value = false;
  invoke("update_download_cmd").catch((e) => {
    const err = e as Partial<UpdateError>;
    updateResult.value = err?.message ?? `启动升级失败：${String(e)}`;
    updateError.value = true;
  });
}

/** 打开发布页手动兜底。 */
function openReleases() {
  openLink("https://github.com/Ljinzhou/oii_sticker/releases");
}

let unlisteners: (() => void)[] = [];
onMounted(async () => {
  // 重接进行中的更新流程（设置页关闭后后台任务仍在跑）。
  try {
    const snapshot = await invoke<UpdatePhase | null>("update_state_cmd");
    if (snapshot && snapshot.phase !== "idle") applyPhase(snapshot);
  } catch {
    /* 状态读取失败不阻塞页面 */
  }
  unlisteners.push(
    await listen<{ downloaded: number; total: number | null }>("updater://progress", (payload) => {
      upPhase.value = "downloading";
      upDownloaded.value = payload.downloaded;
      upTotal.value = payload.total;
      upProgressPct.value = downloadPercent(payload.downloaded, payload.total);
    }),
  );
  unlisteners.push(await listen<UpdatePhase>("updater://phase", (payload) => applyPhase(payload)));
  unlisteners.push(
    await listen<{ level: string; text: string }>("updater://log", (payload) => {
      void pushLog(payload.level ?? "info", payload.text);
    }),
  );
});
onUnmounted(() => {
  unlisteners.forEach((fn) => fn());
  unlisteners = [];
});

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
          <span>主控台背景透明度</span>
          <input
            type="range"
            min="30"
            max="100"
            :value="Number(settings.get('console_bg_opacity', '94'))"
            @change="(e) => settings.set('console_bg_opacity', (e.target as HTMLInputElement).value)"
          />
          <span class="console-opacity-val">{{ Math.round(Number(settings.get('console_bg_opacity', '94'))) }}%</span>
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
          <select
            :value="settings.get('edit_font_family', 'Microsoft YaHei')"
            @change="(e) => settings.set('edit_font_family', (e.target as HTMLSelectElement).value)"
          >
            <option
              v-if="!FONT_OPTIONS.some((f) => f.value === settings.get('edit_font_family', 'Microsoft YaHei'))"
              :value="settings.get('edit_font_family', 'Microsoft YaHei')"
            >当前：{{ settings.get('edit_font_family', 'Microsoft YaHei') }}</option>
            <option v-for="f in FONT_OPTIONS" :key="f.value" :value="f.value">{{ f.label }}</option>
          </select>
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
        <h3>Todo 设置</h3>
        <label class="row">
          <span>Todo 窗口默认置顶</span>
          <input
            type="checkbox"
            :checked="settings.get('default_todo_always_on_top', '1') === '1'"
            @change="(e) => settings.set('default_todo_always_on_top', (e.target as HTMLInputElement).checked ? '1' : '0')"
          />
        </label>
        <TodoPresetsManager kind="reminders" title="提醒时间预设" hint="用于 Todo 详情「提醒时间」行" />
        <TodoPresetsManager kind="due" title="截至时间预设" hint="用于 Todo 详情「截至时间」行" />
        <TodoPresetsManager kind="repeats" title="重复预设" hint="驱动每日自动重建与逾期改名" />
      </div>

      <!-- 工作空间 -->
      <div v-else-if="activeMenu === 'workspace'">
        <WorkspaceManager />
      </div>

      <!-- 关于 -->
      <div v-else-if="activeMenu === 'about'">
        <h3>关于</h3>

        <h4 class="about-sec">基本信息</h4>
        <p class="about-line"><strong>oii_sticker</strong> &nbsp;<span class="badge">v{{ appVersion }}</span></p>

        <h4 class="about-sec">更新检查</h4>
        <div class="about-actions">
          <!-- 主按钮：随更新状态机变形 -->
          <button
            v-if="upPhase === 'available'"
            class="btn primary"
            @click="startUpgrade"
          >
            <i class="ri-download-cloud-2-line"></i>升级到 v{{ upVersion }}
          </button>
          <button
            v-else-if="upPhase === 'downloading'"
            class="btn"
            disabled
          >
            <i class="ri-loader-4-line ri-spin"></i>{{ upRetrying ? "切换镜像重试中…" : "下载更新中…" }}{{ upProgressPct !== null ? ` ${upProgressPct}%` : "" }}
          </button>
          <button
            v-else-if="upPhase === 'installing'"
            class="btn"
            disabled
          >
            <i class="ri-loader-4-line ri-spin"></i>正在安装…
          </button>
          <button
            v-else-if="upPhase === 'restarting'"
            class="btn"
            disabled
          >
            <i class="ri-restart-line"></i>即将重启…
          </button>
          <button
            v-else
            class="btn"
            :disabled="checkingUpdate || upPhase === 'checking'"
            @click="checkUpdate"
          >
            <i class="ri-refresh-line"></i>{{ checkingUpdate || upPhase === "checking" ? "检查中…" : "检查更新" }}
          </button>
          <!-- 失败时的手动兜底 -->
          <button v-if="upPhase === 'failed'" class="btn" @click="openReleases">
            <i class="ri-external-link-line"></i>打开发布页手动下载
          </button>
          <button class="btn" @click="openLink('https://github.com/Ljinzhou/oii_sticker/releases')">
            <i class="ri-download-2-line"></i>查看仓库 / 下载
          </button>
        </div>
        <p
          v-if="upPhase === 'downloading' && updateResult === ''"
          class="result"
        >{{ upRetrying ? "网络不稳定，已自动切换到更快的镜像继续。" : "正在通过最快的镜像下载更新包…" }}</p>
        <p v-if="updateResult" class="result" :class="{ error: updateError }">{{ updateResult }}</p>

        <!-- 更新日志 + 整体进度条（方向 A：贴合现状） -->
        <div class="update-panel">
          <div class="up-bar"><i :class="{ busy: upBarWidth === null && upPhase === 'checking' }" :style="upBarWidth !== null ? { width: upBarWidth + '%' } : {}"></i></div>
          <div class="up-meta"><span>{{ updateStage }}</span><span v-if="upBarWidth !== null">{{ upBarWidth }}%</span></div>
          <div class="up-log" ref="upLogEl">
            <p v-if="upLogs.length === 0" class="l-t0">等待操作：点击「检查更新」查看连接检查、镜像选择与下载日志。</p>
            <div v-for="(l, i) in upLogs" :key="i" :class="'l-' + l.level">{{ l.text }}</div>
          </div>
        </div>

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

.console-opacity-val {
  flex: none;
  min-width: 44px;
  text-align: right;
  font-size: 12px;
  color: #666;
}

/* 更新日志 + 进度条（方向 A：贴合现状） */
.update-panel {
  margin-top: 10px;
  padding: 10px 12px;
  background: #fff;
  border: 1px solid rgba(0, 0, 0, 0.1);
  border-radius: 10px;
}
.up-bar {
  height: 6px;
  background: rgba(0, 0, 0, 0.07);
  border-radius: 4px;
  overflow: hidden;
}
.up-bar i {
  display: block;
  height: 100%;
  width: 0;
  border-radius: 4px;
  background: #4f7cff;
  transition: width 0.25s ease;
}
/* 检查阶段的流动动画（未知总量） */
.up-bar i.busy {
  width: 100%;
  background: repeating-linear-gradient(-45deg, #7ba3ff 0 10px, #4f7cff 10px 20px);
  animation: up-flow 0.8s linear infinite;
}
@keyframes up-flow {
  to { background-position: 28px 0; }
}
.up-meta {
  display: flex;
  justify-content: space-between;
  font-size: 11px;
  color: #888;
  margin-top: 5px;
}
.up-log {
  margin-top: 8px;
  height: 138px;
  box-sizing: border-box;
  overflow-y: auto;
  background: #faf8f4;
  border: 1px solid rgba(0, 0, 0, 0.08);
  border-radius: 8px;
  padding: 7px 10px;
  font-size: 12px;
  line-height: 1.85;
  color: #555;
  font-family: Consolas, "Cascadia Mono", "Courier New", monospace;
}
.up-log p { margin: 0; }
.up-log .l-t0 { color: #aaa; }
.up-log .l-info { color: #555; }
.up-log .l-ok { color: #2f9e5f; }
.up-log .l-warn { color: #d99a1b; }
.up-log .l-err { color: #d33; }

.foot {
  grid-row: 2;
  grid-column: 1 / -1;
  display: flex;
  justify-content: flex-end;
  padding: 10px 18px;
  border-top: 1px solid rgba(0, 0, 0, 0.08);
}
</style>