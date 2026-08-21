<script setup lang="ts">
import { ref, onMounted } from "vue";
import { invoke } from "../../composables/useTauri";

const emit = defineEmits<{ done: [] }>();
const path = ref("");
const name = ref("未命名工作空间");
const removeLegacy = ref(false);
const building = ref(false);
const errorMsg = ref("");

onMounted(async () => {
  try {
    path.value = await invoke<string>("workspace_default_path_cmd");
  } catch {
    path.value = "";
  }
});

async function confirm() {
  if (!path.value.trim()) {
    errorMsg.value = "请填写工作控件目录";
    return;
  }
  building.value = true;
  errorMsg.value = "";
  try {
    await invoke("workspace_bootstrap_cmd", {
      path: path.value.trim(),
      name: name.value || undefined,
      removeLegacy: removeLegacy.value,
    });
    emit("done");
  } catch (e) {
    errorMsg.value = String(e);
  } finally {
    building.value = false;
  }
}
</script>

<template>
  <div class="onboarding">
    <div class="panel">
      <h2>欢迎使用 oii_sticker</h2>
      <p class="hint">创建第一个工作控件以保存便签数据。旧的便签数据会在创建后自动迁移。</p>
      <label class="field">
        <span>目录</span>
        <input v-model="path" class="onboarding-path" type="text" placeholder="工作控件目录" />
      </label>
      <label class="field">
        <span>名称</span>
        <input v-model="name" type="text" placeholder="未命名工作空间" />
      </label>
      <label class="legacy">
        <input v-model="removeLegacy" type="checkbox" />
        <span>迁移后删除旧版数据库（stickers.db）</span>
      </label>
      <p v-if="errorMsg" class="errorMsg">{{ errorMsg }}</p>
      <div class="actions">
        <button class="onboarding-confirm" type="button" :disabled="building" @click="confirm">
          {{ building ? "创建中…" : "确认创建" }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.onboarding {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.4);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 50;
}

.panel {
  width: 340px;
  background: #fff;
  border-radius: 12px;
  padding: 20px;
  box-shadow: 0 12px 40px rgba(0, 0, 0, 0.3);
}

.panel h2 {
  margin: 0 0 6px;
  font-size: 17px;
  color: #333;
}

.hint {
  margin: 0 0 14px;
  font-size: 13px;
  color: #666;
  line-height: 1.6;
}

.field {
  display: block;
  margin-bottom: 12px;
}

.field span {
  display: block;
  font-size: 12px;
  color: #888;
  margin-bottom: 4px;
}

.field input {
  width: 100%;
  box-sizing: border-box;
  border: 1px solid rgba(0, 0, 0, 0.15);
  border-radius: 8px;
  padding: 7px 10px;
  font-size: 13px;
}

.legacy {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: #666;
  margin-bottom: 10px;
}

.errorMsg {
  color: #d33;
  font-size: 12px;
  margin: 0 0 10px;
}

.actions {
  display: flex;
  justify-content: flex-end;
}

.onboarding-confirm {
  background: #4f7cff;
  border: none;
  color: #fff;
  border-radius: 8px;
  padding: 8px 16px;
  font-size: 13px;
  cursor: pointer;
}

.onboarding-confirm:hover {
  background: #3b67e8;
}

.onboarding-confirm:disabled {
  opacity: 0.6;
  cursor: default;
}
</style>
