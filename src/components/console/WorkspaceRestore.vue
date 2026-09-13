<script setup lang="ts">
// 从备份恢复：选 zip → 看备份摘要 → 选恢复方式 → （覆盖需二次确认）→ 执行 → 报告回滚点。
// 自包含组件：入口按钮 + 模态面板都在这里，父组件只需监听 done 刷新列表。
import { ref, computed } from "vue";
import { open as openFile } from "@tauri-apps/plugin-dialog";
import { invoke } from "../../composables/useTauri";
import type { BackupInfo, RestoreOutcome, WorkspaceEntry } from "../../types";

const props = defineProps<{
  workspaces: WorkspaceEntry[];
  currentId: string | null;
  /** 父级正在执行其它工作空间操作（刷新/切换/转移…）时，入口按钮同样禁用，避免并发改动。 */
  parentBusy?: boolean;
}>();

const emit = defineEmits<{ done: [] }>();

const visible = ref(false);
const busy = ref(false);
const error = ref("");

/** 选中的备份与其摘要 */
const zipPath = ref("");
const info = ref<BackupInfo | null>(null);

/** 恢复方式：new（新工作空间） / overwrite（覆盖现有） */
const mode = ref<"new" | "overwrite">("new");
const destRoot = ref("");
/** 覆盖目标（缺省当前工作空间） */
const overwriteId = ref("");
/** 覆盖的二次确认（第一次点击只把按钮变成确认态） */
const armed = ref(false);

const result = ref<RestoreOutcome | null>(null);

const canSubmit = computed(() => {
  if (busy.value || !info.value) return false;
  if (mode.value === "new") return destRoot.value.trim().length > 0;
  return overwriteId.value.length > 0;
});

const overwriteTarget = computed(() =>
  props.workspaces.find((w) => w.id === overwriteId.value) ?? null,
);

function reset() {
  zipPath.value = "";
  info.value = null;
  error.value = "";
  result.value = null;
  armed.value = false;
  mode.value = "new";
  destRoot.value = "";
  overwriteId.value = props.currentId ?? "";
}

function open() {
  reset();
  visible.value = true;
}

function close() {
  if (busy.value) return;
  visible.value = false;
}

async function pickZip() {
  error.value = "";
  const picked = await openFile({
    multiple: false,
    directory: false,
    title: "选择备份文件",
    filters: [{ name: "工作空间备份", extensions: ["zip"] }],
  });
  if (!picked || Array.isArray(picked)) return;
  zipPath.value = picked;
  busy.value = true;
  try {
    info.value = await invoke<BackupInfo>("workspace_inspect_backup_cmd", { zipPath: picked });
  } catch (e) {
    info.value = null;
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

async function pickDest() {
  const dir = await openFile({ directory: true, title: "选择恢复到的文件夹（需为空目录）" });
  if (!dir || Array.isArray(dir)) return;
  destRoot.value = dir;
}

async function submit() {
  if (!canSubmit.value) return;
  if (mode.value === "overwrite" && !armed.value) {
    armed.value = true; // 第一次点击 → 变成「确认覆盖」
    return;
  }
  busy.value = true;
  error.value = "";
  try {
    result.value = await invoke<RestoreOutcome>("workspace_restore_cmd", {
      zipPath: zipPath.value,
      mode: mode.value,
      destRoot: mode.value === "new" ? destRoot.value.trim() : null,
      id: mode.value === "overwrite" ? overwriteId.value : null,
      name: null,
    });
    emit("done");
  } catch (e) {
    error.value = String(e);
    armed.value = false;
  } finally {
    busy.value = false;
  }
}

function formatSize(bytes: number): string {
  if (!Number.isFinite(bytes)) return String(bytes);
  if (bytes < 1024) return `${bytes} B`;
  const kb = bytes / 1024;
  if (kb < 1024) return `${kb.toFixed(1)} KB`;
  return `${(kb / 1024).toFixed(1)} MB`;
}

function formatTime(ms: number): string {
  if (!ms) return "未知";
  const d = new Date(ms);
  if (Number.isNaN(d.getTime())) return "未知";
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}
</script>

<template>
  <button class="ws-btn wsr-trigger" :disabled="busy || parentBusy" @click="open">
    <i class="ri-history-line"></i>从备份恢复
  </button>

  <div v-if="visible" class="wsr-mask" @click.self="close">
    <div class="wsr-panel" role="dialog" aria-modal="true" aria-label="从备份恢复">
      <header class="wsr-head">
        <h4 class="wsr-title">从备份恢复</h4>
        <button class="wsr-x" :disabled="busy" aria-label="关闭" @click="close">
          <i class="ri-close-line"></i>
        </button>
      </header>

      <!-- 结果态 -->
      <template v-if="result">
        <section class="wsr-done" :class="{ danger: result.mode === 'overwrite' }">
          <p class="wsr-done-title">
            已恢复「{{ result.name }}」· {{ result.entries }} 个文件
          </p>
          <p class="wsr-done-path">{{ result.root }}</p>
          <p v-if="result.rollback_zip" class="wsr-done-hint">
            恢复前的完整备份（可据此回退）：<br />{{ result.rollback_zip }}
          </p>
          <p v-if="result.rollback_dir" class="wsr-done-hint">
            旧数据已改名保留在：<br />{{ result.rollback_dir }}
          </p>
          <p v-if="result.mode === 'new'" class="wsr-done-hint">
            新工作空间已加入列表，可在列表中「切换」过去。
          </p>
        </section>
        <footer class="wsr-foot">
          <button class="ws-btn primary wsr-finish" @click="close">完成</button>
        </footer>
      </template>

      <template v-else>
        <!-- 1. 选文件 -->
        <section class="wsr-step">
          <div class="wsr-step-no">1</div>
          <div class="wsr-step-body">
            <p class="wsr-step-title">选择备份文件</p>
            <p class="wsr-hint">由本程序「备份」导出的 zip 文件。</p>
            <div class="wsr-pick-row">
              <button class="ws-btn wsr-pick" :disabled="busy" @click="pickZip">
                <span v-if="busy && !info" class="spin"></span>选择备份文件…
              </button>
              <span v-if="zipPath" class="wsr-file">{{ zipPath }}</span>
            </div>
          </div>
        </section>

        <!-- 2. 摘要 + 方式 -->
        <template v-if="info">
          <section class="wsr-step">
            <div class="wsr-step-no">2</div>
            <div class="wsr-step-body">
              <p class="wsr-step-title">备份内容</p>
              <dl class="wsr-meta">
                <div class="wsr-meta-item">
                  <dt>工作空间</dt>
                  <dd>{{ info.name }}</dd>
                </div>
                <div class="wsr-meta-item">
                  <dt>备份时间</dt>
                  <dd>{{ formatTime(info.backup_at_ms) }}</dd>
                </div>
                <div class="wsr-meta-item">
                  <dt>文件</dt>
                  <dd>{{ info.entries }} 个 · {{ formatSize(info.bytes) }}</dd>
                </div>
                <div class="wsr-meta-item">
                  <dt>归档</dt>
                  <dd>{{ formatSize(info.zip_bytes) }}</dd>
                </div>
              </dl>
              <p v-if="!info.has_manifest" class="wsr-hint warn">
                旧版备份（无摘要信息），备份时间取自文件修改时间。
              </p>
            </div>
          </section>

          <section class="wsr-step">
            <div class="wsr-step-no">3</div>
            <div class="wsr-step-body">
              <p class="wsr-step-title">恢复方式</p>
              <div class="wsr-modes">
                <button
                  class="wsr-mode"
                  :class="{ on: mode === 'new' }"
                  :disabled="busy"
                  @click="mode = 'new'; armed = false"
                >
                  <span class="wsr-mode-name">恢复为新工作空间</span>
                  <span class="wsr-mode-desc">保留现有数据，恢复出的内容作为新的工作空间加入列表</span>
                </button>
                <button
                  class="wsr-mode"
                  :class="{ on: mode === 'overwrite', danger: true }"
                  :disabled="busy || props.workspaces.length === 0"
                  @click="mode = 'overwrite'; armed = false"
                >
                  <span class="wsr-mode-name">覆盖现有工作空间</span>
                  <span class="wsr-mode-desc">用备份替换该工作空间的内容（会先自动生成回滚点）</span>
                </button>
              </div>

              <!-- 目标：新工作空间 → 选目录 -->
              <div v-if="mode === 'new'" class="wsr-target">
                <button class="ws-btn wsr-pick" :disabled="busy" @click="pickDest">选择目标文件夹…</button>
                <span v-if="destRoot" class="wsr-file">{{ destRoot }}</span>
                <p class="wsr-hint">目标文件夹需为空（不存在会自动创建）。</p>
              </div>

              <!-- 目标：覆盖 → 选工作空间 -->
              <div v-else class="wsr-target">
                <label class="wsr-label" for="wsr-overwrite">要覆盖的工作空间</label>
                <select id="wsr-overwrite" v-model="overwriteId" class="wsr-select" :disabled="busy">
                  <option v-for="w in props.workspaces" :key="w.id" :value="w.id">
                    {{ w.name }}{{ w.id === props.currentId ? "（当前）" : "" }}
                  </option>
                </select>
                <p class="wsr-hint">
                  恢复前会：① 把该工作空间完整备份到 cache/recovery-*.zip；② 关闭其便签窗口；
                  ③ 旧数据改名保留到 cache/rollback-*（不删除）。
                </p>
              </div>

              <p v-if="overwriteTarget && overwriteTarget.id === props.currentId" class="wsr-hint warn">
                目标正是当前工作空间：所有便签窗口会被关闭，恢复后请重新打开便签。
              </p>
            </div>
          </section>
        </template>

        <p v-if="error" class="wsr-error">{{ error }}</p>

        <footer class="wsr-foot">
          <button class="ws-btn" :disabled="busy" @click="close">取消</button>
          <button
            class="ws-btn wsr-submit"
            :class="{ primary: !armed, danger: armed }"
            :disabled="!canSubmit"
            @click="submit"
          >
            <span v-if="busy" class="spin"></span>
            <template v-if="mode === 'overwrite' && armed">确认覆盖，开始恢复</template>
            <template v-else>开始恢复</template>
          </button>
        </footer>
      </template>
    </div>
  </div>
</template>

<style scoped>
.wsr-trigger {
  gap: 6px;
}

.wsr-trigger .ri {
  font-size: 14px;
  vertical-align: -1px;
}

.wsr-mask {
  position: fixed;
  inset: 0;
  z-index: 70;
  display: grid;
  place-items: center;
  background: rgba(20, 16, 8, 0.35);
  animation: wsr-fade 140ms ease;
}

@keyframes wsr-fade {
  from {
    opacity: 0;
  }
}

.wsr-panel {
  width: min(520px, 92vw);
  max-height: 86vh;
  overflow: auto;
  box-sizing: border-box;
  padding: 16px 18px 14px;
  border-radius: 12px;
  background: #fffdf8;
  color: #333;
  font-family: system-ui, "Microsoft YaHei", "PingFang SC", sans-serif;
  font-size: 13px;
  box-shadow: 0 16px 44px rgba(0, 0, 0, 0.24);
  animation: wsr-in 150ms cubic-bezier(0.32, 0.72, 0, 1);
}

@keyframes wsr-in {
  from {
    opacity: 0;
    transform: translateY(6px) scale(0.99);
  }
}

.wsr-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 12px;
}

.wsr-title {
  margin: 0;
  font-size: 14px;
  font-weight: 600;
  letter-spacing: 0.5px;
}

.wsr-x {
  border: none;
  background: none;
  color: #999;
  cursor: pointer;
  padding: 2px 4px;
  border-radius: 6px;
  font-size: 15px;
  line-height: 1;
}

.wsr-x:hover:not(:disabled) {
  background: rgba(0, 0, 0, 0.05);
  color: #333;
}

.wsr-step {
  display: flex;
  gap: 10px;
  padding: 10px 0;
  border-top: 1px dashed rgba(0, 0, 0, 0.08);
}

.wsr-step-no {
  flex: none;
  width: 18px;
  height: 18px;
  border-radius: 50%;
  background: rgba(79, 124, 255, 0.12);
  color: #3b63d6;
  font-size: 11px;
  font-weight: 700;
  display: grid;
  place-items: center;
  margin-top: 1px;
}

.wsr-step-body {
  flex: 1;
  min-width: 0;
}

.wsr-step-title {
  margin: 0 0 4px;
  font-size: 13px;
  font-weight: 600;
}

.wsr-hint {
  margin: 4px 0 0;
  font-size: 11.5px;
  color: #999;
  line-height: 1.6;
}

.wsr-hint.warn {
  color: #b4791f;
}

.wsr-pick-row {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.wsr-pick {
  gap: 6px;
}

.wsr-file {
  font-size: 11.5px;
  color: #666;
  overflow-wrap: anywhere;
  background: #fbf7ec;
  border: 1px dashed rgba(0, 0, 0, 0.1);
  border-radius: 6px;
  padding: 3px 8px;
}

.wsr-meta {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 6px 12px;
  margin: 6px 0 0;
}

.wsr-meta-item dt {
  font-size: 11px;
  color: #999;
}

.wsr-meta-item dd {
  margin: 0;
  font-size: 12.5px;
  font-weight: 600;
  overflow-wrap: anywhere;
}

.wsr-modes {
  display: grid;
  gap: 6px;
  margin-top: 6px;
}

.wsr-mode {
  text-align: left;
  border: 1px solid rgba(0, 0, 0, 0.1);
  border-radius: 9px;
  background: #fff;
  padding: 8px 11px;
  cursor: pointer;
  display: grid;
  gap: 2px;
  transition: border-color 120ms ease, background 120ms ease;
}

.wsr-mode:hover:not(:disabled) {
  border-color: rgba(79, 124, 255, 0.4);
}

.wsr-mode.on {
  border-color: #4f7cff;
  background: rgba(79, 124, 255, 0.07);
}

.wsr-mode.on.danger {
  border-color: #d33;
  background: rgba(221, 51, 51, 0.06);
}

.wsr-mode:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

.wsr-mode-name {
  font-size: 12.5px;
  font-weight: 600;
}

.wsr-mode-desc {
  font-size: 11.5px;
  color: #666;
  line-height: 1.55;
}

.wsr-target {
  margin-top: 10px;
  display: grid;
  gap: 6px;
}

.wsr-label {
  font-size: 11.5px;
  color: #999;
}

.wsr-select {
  border: 1px solid rgba(0, 0, 0, 0.1);
  border-radius: 7px;
  padding: 5px 9px;
  font-size: 12.5px;
  background: #fff;
  color: #333;
}

.wsr-error {
  margin: 8px 0 0;
  padding: 7px 10px;
  border-radius: 7px;
  font-size: 12px;
  color: #d33;
  background: rgba(221, 51, 51, 0.08);
  border: 1px solid rgba(221, 51, 51, 0.25);
  overflow-wrap: anywhere;
}

.wsr-done {
  border: 1px solid rgba(46, 158, 91, 0.28);
  background: rgba(46, 158, 91, 0.07);
  border-radius: 9px;
  padding: 12px 14px;
}

.wsr-done.danger {
  border-color: rgba(180, 121, 31, 0.3);
  background: rgba(180, 121, 31, 0.07);
}

.wsr-done-title {
  margin: 0 0 6px;
  font-weight: 600;
}

.wsr-done-path {
  margin: 0;
  font-size: 12px;
  color: #666;
  overflow-wrap: anywhere;
}

.wsr-done-hint {
  margin: 8px 0 0;
  font-size: 11.5px;
  color: #666;
  line-height: 1.6;
  overflow-wrap: anywhere;
}

.wsr-foot {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 12px;
}

/* 与父级一致的基础按钮（scoped 样式不跨组件，故此处自带一份） */
.ws-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  border: 1px solid rgba(0, 0, 0, 0.1);
  background: #fff;
  color: #333;
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
  background: #4f7cff;
  border-color: #4f7cff;
  color: #fff;
}

.ws-btn.primary:hover:not(:disabled) {
  background: #3e6bf0;
}

.ws-btn.danger {
  background: #d33;
  border-color: #d33;
  color: #fff;
}

.ws-btn.danger:hover:not(:disabled) {
  background: #c02c2c;
}

.spin {
  width: 10px;
  height: 10px;
  display: inline-block;
  border: 2px solid rgba(255, 255, 255, 0.35);
  border-top-color: #fff;
  border-radius: 50%;
  animation: wsr-spin 0.6s linear infinite;
}

.ws-btn:not(.primary):not(.danger) .spin {
  border-color: rgba(0, 0, 0, 0.12);
  border-top-color: #4f7cff;
}

@keyframes wsr-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
