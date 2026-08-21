<script setup lang="ts">
import { ref, computed, onMounted } from "vue";
import { invoke } from "../../composables/useTauri";
import type { WorkspaceEntry } from "../../types";

// —— 数据 ——
const entries = ref<WorkspaceEntry[]>([]);
const currentId = ref<string | null>(null);
const loading = ref(true);

// 忙碌状态（同一时刻只允许一个 async 操作；busyKey 标识具体操作以显示 spinner）
const busyKey = ref<string | null>(null);
const busy = computed(() => busyKey.value !== null);

const toast = ref<{ type: "error" | "ok"; text: string } | null>(null);
let toastTimer: ReturnType<typeof setTimeout> | null = null;

// 新建
const creating = ref(false);
const newPath = ref("");
const newName = ref("");

// 备份（仅当前工作控件，hero 卡内展开）
const backupOpen = ref(false);
const destZip = ref("");
const backupResult = ref("");
let defaultRoot = "";

// 转移（hero 与列表行共用，按 id 展开）
const transferId = ref<string | null>(null);
const destRoot = ref("");

const current = computed(() => entries.value.find((e) => e.id === currentId.value) ?? null);

function showToast(type: "error" | "ok", text: string) {
  if (toastTimer) clearTimeout(toastTimer);
  toast.value = { type, text };
  toastTimer = setTimeout(() => (toast.value = null), 4000);
}

function showError(e: unknown) {
  showToast("error", String(e));
}

function showOk(text: string) {
  showToast("ok", text);
}

async function refresh() {
  busyKey.value = "refresh";
  try {
    const [list, cur] = await Promise.all([
      invoke<WorkspaceEntry[]>("workspace_list_cmd"),
      invoke<WorkspaceEntry | null>("workspace_current_cmd"),
    ]);
    entries.value = list;
    currentId.value = cur?.id ?? null;
  } catch (e) {
    showError(e);
  } finally {
    busyKey.value = null;
  }
}

onMounted(async () => {
  try {
    defaultRoot = await invoke<string>("workspace_default_path_cmd");
  } catch {
    defaultRoot = "";
  }
  await refresh();
  loading.value = false;
});

// —— 新建 ——
function openCreate() {
  creating.value = true;
}

function cancelCreate() {
  creating.value = false;
  newPath.value = "";
  newName.value = "";
}

async function onCreate() {
  const path = newPath.value.trim();
  if (!path) return;
  busyKey.value = "create";
  try {
    const created = await invoke<WorkspaceEntry>("workspace_create_cmd", {
      path,
      name: newName.value.trim() || null,
    });
    showOk(`已创建「${created.name}」`);
    cancelCreate();
    await refresh();
  } catch (e) {
    showError(e);
  } finally {
    busyKey.value = null;
  }
}

// —— 切换 ——
async function onSwitch(row: WorkspaceEntry) {
  const ok = window.confirm(`切换到「${row.name}」？所有便签窗口将被关闭，请确认。`);
  if (!ok) return;
  busyKey.value = `switch:${row.id}`;
  try {
    await invoke("workspace_switch_cmd", { id: row.id });
    showOk(`已切换到「${row.name}」`);
    await refresh();
  } catch (e) {
    showError(e);
  } finally {
    busyKey.value = null;
  }
}

// —— 销毁 ——
async function onDestroy(row: WorkspaceEntry) {
  const ok = window.confirm(
    `确定销毁「${row.name}」？该控件的全部便签数据将被删除，无法恢复。`,
  );
  if (!ok) return;
  busyKey.value = `destroy:${row.id}`;
  try {
    await invoke("workspace_destroy_cmd", { id: row.id });
    showOk(`已销毁「${row.name}」`);
    await refresh();
  } catch (e) {
    showError(e);
  } finally {
    busyKey.value = null;
  }
}

// —— 备份（当前工作控件）——
function openBackup() {
  backupOpen.value = true;
  backupResult.value = "";
  if (!destZip.value) destZip.value = defaultRoot ? `${defaultRoot}.zip` : "oiistiker_workspace.zip";
}

function formatSize(bytes: number): string {
  if (!Number.isFinite(bytes)) return String(bytes);
  if (bytes < 1024) return `${bytes} B`;
  const kb = bytes / 1024;
  if (kb < 1024) return `${kb.toFixed(1)} KB`;
  return `${(kb / 1024).toFixed(1)} MB`;
}

async function runBackup() {
  const dest = destZip.value.trim();
  if (!dest || !currentId.value) return;
  busyKey.value = "backup";
  try {
    const size = await invoke<number>("workspace_backup_cmd", { id: currentId.value, destZip: dest });
    backupResult.value = `备份完成：${formatSize(size)}`;
  } catch (e) {
    showError(e);
  } finally {
    busyKey.value = null;
  }
}

// —— 转移 ——
function openTransfer(id: string) {
  transferId.value = id;
  destRoot.value = "";
}

async function runTransfer() {
  const dest = destRoot.value.trim();
  if (!dest || !transferId.value) return;
  busyKey.value = `transfer:${transferId.value}`;
  try {
    await invoke<void>("workspace_transfer_cmd", {
      id: transferId.value,
      destRoot: dest,
    });
    showOk("转移完成");
    transferId.value = null;
    await refresh();
  } catch (e) {
    showError(e);
  } finally {
    busyKey.value = null;
  }
}
</script>

<template>
  <div class="ws-manager">
    <div v-if="toast" class="ws-toast" :class="toast.type">{{ toast.text }}</div>

    <!-- 当前工作控件 hero 卡 -->
    <section class="ws-hero">
      <header class="ws-hero-head">
        <h3 class="ws-hero-kicker">当前工作控件</h3>
        <span v-if="current" class="ws-badge">当前</span>
      </header>

      <template v-if="current">
        <div class="ws-hero-name">{{ current.name }}</div>
        <div class="ws-hero-path">{{ current.path }}</div>
        <div class="ws-hero-actions">
          <button class="ws-btn primary hero-backup" :disabled="busy" @click="openBackup">
            <span v-if="busyKey === 'backup'" class="spin"></span>备份
          </button>
          <button class="ws-btn hero-transfer" :disabled="busy" @click="openTransfer(current.id)">
            转移
          </button>
        </div>

        <div v-if="backupOpen" class="ws-inline">
          <input v-model="destZip" class="ws-input backup-input" placeholder="备份 zip 的完整路径，如 C:/Users/you/Documents/oiistiker_workspace.zip" />
          <div class="ws-inline-actions">
            <button class="ws-btn primary backup-run" :disabled="busy || !destZip.trim()" @click="runBackup">
              <span v-if="busyKey === 'backup'" class="spin"></span>执行备份
            </button>
            <button class="ws-btn" :disabled="busy" @click="backupOpen = false">取消</button>
          </div>
          <!-- TODO: 系统目录选择对话框（tauri dialog plugin）未安装，暂用文本输入替代 -->
          <p class="ws-hint">暂未接入目录选择对话框，请直接输入完整 zip 路径。</p>
          <p v-if="backupResult" class="ws-ok-line">{{ backupResult }}</p>
        </div>
      </template>
      <p v-else class="ws-empty">未设置当前工作控件，请从下方列表新建。</p>
    </section>

    <!-- 全部工作控件列表 -->
    <section class="ws-list">
      <header class="ws-list-head">
        <span class="ws-list-title">全部工作控件</span>
        <button class="ws-btn ghost ws-refresh" :disabled="busy" @click="refresh">
          <span v-if="busyKey === 'refresh'" class="spin"></span>刷新
        </button>
      </header>

      <div v-if="loading" class="ws-loading">加载中…</div>
      <template v-else>
        <div
          v-for="w in entries"
          :key="w.id"
          class="ws-row"
          :class="{ active: w.id === currentId }"
        >
          <div class="ws-row-main">
            <div class="ws-row-name">
              <span>{{ w.name }}</span>
              <span v-if="w.id === currentId" class="ws-badge">当前</span>
            </div>
            <div class="ws-row-meta">{{ w.path }}</div>
          </div>
          <div class="ws-row-actions">
            <button
              v-if="w.id !== currentId"
              class="ws-btn sm row-switch"
              :disabled="busy"
              @click="onSwitch(w)"
            ><span v-if="busyKey === `switch:${w.id}`" class="spin"></span>切换</button>
            <button class="ws-btn sm row-transfer" :disabled="busy" @click="openTransfer(w.id)">
              转移
            </button>
            <button class="ws-btn sm danger row-destroy" :disabled="busy" @click="onDestroy(w)">
              <span v-if="busyKey === `destroy:${w.id}`" class="spin"></span>销毁
            </button>
          </div>

          <div v-if="transferId === w.id" class="ws-inline">
            <input v-model="destRoot" class="ws-input transfer-input" placeholder="转移目标目录（完整路径，如 C:/Users/you/Documents/workspace_new" />
            <div class="ws-inline-actions">
              <button class="ws-btn primary sm transfer-run" :disabled="busy || !destRoot.trim()" @click="runTransfer">
                <span v-if="busyKey === `transfer:${w.id}`" class="spin"></span>执行转移
              </button>
              <button class="ws-btn sm" :disabled="busy" @click="transferId = null">取消</button>
            </div>
          </div>
        </div>

        <div v-if="entries.length === 0" class="ws-empty">还没有工作控件，点击下方新建。</div>

        <!-- 新建（虚线卡片） -->
        <div class="ws-new">
          <button v-if="!creating" class="ws-new-trigger" :disabled="busy" @click="openCreate">
            + 新建工作控件
          </button>
          <div v-else class="ws-inline">
            <input v-model="newPath" class="ws-input new-path-input" placeholder="存放路径（完整目录，如 C:/Users/you/Documents/workspace1" />
            <input v-model="newName" class="ws-input new-name-input" placeholder="名称（选填，默认“未命名工作空间”）" />
            <div class="ws-inline-actions">
              <button class="ws-btn primary new-run" :disabled="busy || !newPath.trim()" @click="onCreate">
                <span v-if="busyKey === 'create'" class="spin"></span>创建
              </button>
              <button class="ws-btn" :disabled="busy" @click="cancelCreate">取消</button>
            </div>
          </div>
        </div>
      </template>
    </section>
  </div>
</template>

<style scoped>
.ws-manager {
  --paper: #fffdf8;
  --paper-card: #ffffff;
  --paper-deep: #fbf7ec;
  --ink: #333;
  --ink-2: #666;
  --ink-3: #999;
  --accent: #4f7cff;
  --accent-soft: rgba(79, 124, 255, 0.12);
  --danger: #d33;
  --ok: #2e9e5b;
  --hairline: rgba(0, 0, 0, 0.06);
  --hairline-strong: rgba(0, 0, 0, 0.1);
  --radius: 10px;
  max-width: 640px;
  font-family: system-ui, "Microsoft YaHei", "PingFang SC", sans-serif;
  font-size: 13px;
  color: var(--ink);
  animation: ws-in 160ms ease;
}

@keyframes ws-in {
  from {
    opacity: 0;
    transform: translateY(4px);
  }
}

/* —— toast —— */
.ws-toast {
  position: sticky;
  top: 4px;
  z-index: 6;
  margin-bottom: 10px;
  padding: 8px 12px;
  border-radius: 7px;
  font-size: 12px;
  animation: ws-in 140ms ease;
}

.ws-toast.error {
  background: rgba(221, 51, 51, 0.08);
  border: 1px solid rgba(221, 51, 51, 0.28);
  color: var(--danger);
}

.ws-toast.ok {
  background: rgba(46, 158, 91, 0.08);
  border: 1px solid rgba(46, 158, 91, 0.28);
  color: var(--ok);
}

/* —— hero 卡 —— */
.ws-hero {
  background: var(--paper-card);
  border: 1px solid var(--hairline-strong);
  border-radius: var(--radius);
  padding: 16px 18px;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.03), 0 6px 18px rgba(0, 0, 0, 0.04);
}

.ws-hero-head {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 8px;
}

.ws-hero-kicker {
  margin: 0;
  font-size: 12px;
  letter-spacing: 1.5px;
  color: var(--ink-3);
  font-weight: 600;
}

.ws-hero-name {
  font-size: 16px;
  font-weight: 600;
  color: var(--ink);
}

.ws-hero-path {
  font-size: 12px;
  color: var(--ink-3);
  margin: 4px 0 12px;
  word-break: break-all;
}

.ws-hero-actions {
  display: flex;
  gap: 8px;
}

/* —— 徽标 —— */
.ws-badge {
  display: inline-flex;
  align-items: center;
  font-size: 11px;
  font-weight: 600;
  color: var(--ok);
  background: rgba(46, 158, 91, 0.12);
  border: 1px solid rgba(46, 158, 91, 0.25);
  border-radius: 4px;
  padding: 1px 7px;
  line-height: 16px;
}

/* —— 按钮 —— */
.ws-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  border: 1px solid var(--hairline-strong);
  background: var(--paper-card);
  color: var(--ink);
  border-radius: 7px;
  padding: 6px 12px;
  font-size: 12px;
  cursor: pointer;
  transition: background 120ms ease, border-color 120ms ease, color 120ms ease;
}

.ws-btn:hover:not(:disabled) {
  background: #f2f4f7;
}

.ws-btn:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

.ws-btn.primary {
  background: var(--accent);
  border-color: var(--accent);
  color: #fff;
}

.ws-btn.primary:hover:not(:disabled) {
  background: #3e6bf0;
}

.ws-btn.danger {
  color: var(--danger);
  border-color: rgba(221, 51, 51, 0.3);
  background: var(--paper-card);
}

.ws-btn.danger:hover:not(:disabled) {
  background: rgba(221, 51, 51, 0.06);
}

.ws-btn.ghost {
  border: none;
  background: none;
  color: var(--ink-3);
}

.ws-btn.ghost:hover:not(:disabled) {
  background: rgba(0, 0, 0, 0.05);
  color: var(--ink);
}

.ws-btn.sm {
  padding: 3px 9px;
  font-size: 12px;
}

/* —— 列表 —— */
.ws-list {
  margin-top: 20px;
}

.ws-list-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 8px;
}

.ws-list-title {
  font-size: 12px;
  letter-spacing: 1.5px;
  color: var(--ink-3);
  font-weight: 600;
}

.ws-row {
  background: var(--paper-card);
  border: 1px solid var(--hairline);
  border-left: 3px solid transparent;
  border-radius: var(--radius);
  padding: 10px 14px;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 8px;
  transition: background 120ms ease, border-color 120ms ease;
}

.ws-row.active {
  background: var(--accent-soft);
  border-color: rgba(79, 124, 255, 0.25);
  border-left-color: var(--accent);
}

.ws-row-name {
  font-size: 14px;
  font-weight: 600;
  display: flex;
  gap: 8px;
  align-items: center;
}

.ws-row-meta {
  font-size: 12px;
  color: var(--ink-3);
  margin-top: 2px;
  word-break: break-all;
}

.ws-row-actions {
  display: flex;
  gap: 6px;
}

/* —— 内联表单 —— */
.ws-inline {
  flex-basis: 100%;
  border-top: 1px dashed var(--hairline-strong);
  margin-top: 8px;
  padding-top: 10px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  animation: ws-in 140ms ease;
}

.ws-inline-actions {
  display: flex;
  gap: 8px;
}

.ws-input {
  width: 100%;
  border: 1px solid var(--hairline-strong);
  border-radius: 6px;
  padding: 5px 9px;
  font-size: 12px;
  color: var(--ink);
  background: var(--paper-card);
}

.ws-input:focus {
  outline: none;
  border-color: var(--accent);
  box-shadow: 0 0 0 2px rgba(79, 124, 255, 0.12);
}

.ws-hint {
  font-size: 11px;
  color: var(--ink-3);
  margin: 2px 0 0;
}

.ws-ok-line {
  font-size: 12px;
  color: var(--ok);
  margin: 4px 0 0;
}

/* —— 新建 —— */
.ws-new {
  border: 1px dashed rgba(79, 124, 255, 0.35);
  border-radius: var(--radius);
  padding: 10px 14px;
}

.ws-new-trigger {
  border: none;
  background: none;
  color: var(--accent);
  font-size: 12px;
  cursor: pointer;
  padding: 2px 0;
  width: 100%;
  text-align: left;
}

.ws-new-trigger:hover:not(:disabled) {
  color: #3e6bf0;
}

/* —— 空态 / 加载 —— */
.ws-empty {
  color: var(--ink-3);
  font-size: 12px;
  padding: 8px 0;
}

.ws-loading {
  color: var(--ink-3);
  font-size: 12px;
  padding: 8px 2px;
}

/* —— spinner —— */
.spin {
  width: 10px;
  height: 10px;
  display: inline-block;
  border: 2px solid rgba(255, 255, 255, 0.35);
  border-top-color: #fff;
  border-radius: 50%;
  animation: ws-spin 0.6s linear infinite;
}

.ws-btn:not(.primary) .spin {
  border-color: rgba(0, 0, 0, 0.12);
  border-top-color: var(--accent);
}

@keyframes ws-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
