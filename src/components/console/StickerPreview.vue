<script setup lang="ts">
// 右侧预览面板：显示选中便签的渲染内容与统计；未选中便签时显示所属分组概览。
// 统计全部来自正文（纯函数），不需要额外后端命令。
import { computed } from "vue";
import { renderMarkdown } from "../../utils/markdown";
import { useNotesStore } from "../../stores/notes";
import type { Sticker, StickerGroup } from "../../types";

const props = defineProps<{
  sticker: Sticker | null;
  /** 当前选中的分组（null = 未分组）；未传 = 没有选中分组 */
  group?: StickerGroup | null;
  /** 是否处于「未分组」选中态（group=null 且有值时为 true） */
  groupSelected?: boolean;
  /** 该分组下的便签（分组概览用，含子分组） */
  groupStickers?: Sticker[];
}>();

const emit = defineEmits<{
  edit: [sticker: Sticker];
  remove: [sticker: Sticker];
}>();

const notes = useNotesStore();

/** 正文统计（纯函数）。 */
const stats = computed(() => {
  const text = props.sticker?.content ?? "";
  const headings = (text.match(/^\s{0,3}#{1,6}\s/gm) ?? []).length;
  return {
    chars: text.length,
    noSpace: text.replace(/\s/g, "").length,
    lines: text ? text.split("\n").length : 0,
    todo: (text.match(/^\s*[-*+]\s\[ \]/gm) ?? []).length,
    done: (text.match(/^\s*[-*+]\s\[[xX]\]/gm) ?? []).length,
    headings,
    tables: (text.match(/^\s*\|.*\|\s*$/gm) ?? []).length,
    codes: Math.floor((text.match(/^\s*```/gm) ?? []).length / 2),
    links: (text.match(/\[[^\]]*\]\([^)]*\)/g) ?? []).length,
    images: (text.match(/!\[[^\]]*\]\([^)]*\)/g) ?? []).length,
  };
});

const rendered = computed(() => (props.sticker ? renderMarkdown(props.sticker.content) : ""));

/** 便签所在路径：分组路径 + 标题.md（与磁盘布局一致）。 */
const stickerPath = computed(() => {
  if (!props.sticker) return "";
  const groupPath = props.sticker.group_id != null ? notes.groupPath(props.sticker.group_id) : "";
  const name = (props.sticker.title || "未命名").trim() || "未命名";
  return groupPath ? `stickers/${groupPath}/${name}.md` : `stickers/${name}.md`;
});

/** 分组概览：任务汇总 + 最近更新。 */
const groupSummary = computed(() => {
  const list = props.groupStickers ?? [];
  let todo = 0;
  let done = 0;
  for (const s of list) {
    todo += (s.content.match(/^\s*[-*+]\s\[ \]/gm) ?? []).length;
    done += (s.content.match(/^\s*[-*+]\s\[[xX]\]/gm) ?? []).length;
  }
  const recent = [...list]
    .sort((a, b) => (b.updated_at ?? "").localeCompare(a.updated_at ?? ""))
    .slice(0, 4);
  return { total: list.length, todo, done, recent };
});

const groupPath = computed(() => (props.group ? notes.groupPath(props.group.id) : ""));

function formatSize(chars: number): string {
  if (chars < 1024) return `${chars} B`;
  return `${(chars / 1024).toFixed(1)} KB`;
}
</script>

<template>
  <aside class="preview">
    <!-- 便签预览 -->
    <template v-if="sticker">
      <header class="pv-head">
        <div class="pv-title-wrap">
          <h4 class="pv-title">{{ sticker.title || "（无标题）" }}</h4>
          <p class="pv-path">
            <i class="ri-file-text-line"></i>{{ stickerPath }}
          </p>
        </div>
        <div class="pv-actions">
          <button class="btn small" @click="emit('edit', sticker)">
            <i class="ri-edit-line"></i>编辑
          </button>
          <button class="btn small danger" title="删除便签" @click="emit('remove', sticker)">
            <i class="ri-delete-bin-line"></i>
          </button>
        </div>
      </header>

      <div class="metrics">
        <div class="metric">
          <div class="k">字数</div>
          <div class="v">{{ stats.noSpace }}<small>字</small></div>
        </div>
        <div class="metric">
          <div class="k">字符 / 行</div>
          <div class="v">{{ stats.chars }}<small>/ {{ stats.lines }} 行</small></div>
        </div>
        <div class="metric" :class="{ warn: stats.todo > 0 }">
          <div class="k">待完成任务</div>
          <div class="v">{{ stats.todo }}<small>项</small></div>
        </div>
        <div class="metric" :class="{ ok: stats.done > 0 }">
          <div class="k">已完成任务</div>
          <div class="v">{{ stats.done }}<small>项</small></div>
        </div>
        <div class="metric">
          <div class="k">体积（约）</div>
          <div class="v">{{ formatSize(stats.chars) }}</div>
        </div>
        <div class="metric">
          <div class="k">更新时间</div>
          <div class="v" style="font-size: 12.5px">{{ sticker.updated_at || "—" }}</div>
        </div>
      </div>

      <div class="pv-body">
        <div class="sect-title">正文预览</div>
        <div class="md-body" v-html="rendered"></div>
        <div class="sect-title">结构统计</div>
        <dl class="kv">
          <dt>标题</dt><dd>{{ stats.headings }}</dd>
          <dt>表格行</dt><dd>{{ stats.tables }}</dd>
          <dt>代码块</dt><dd>{{ stats.codes }}</dd>
          <dt>链接 / 图片</dt><dd>{{ stats.links }} / {{ stats.images }}</dd>
          <dt>便签 ID</dt><dd><code>{{ sticker.uid || "（旧数据，升级后自动补齐）" }}</code></dd>
        </dl>
      </div>
    </template>

    <!-- 分组概览 -->
    <template v-else-if="groupSelected">
      <header class="pv-head">
        <div class="pv-title-wrap">
          <h4 class="pv-title">
            <i
              class="ri-folder-open-line"
              :style="{ color: group?.color ?? '#9aa0a6' }"
            ></i>
            {{ group ? group.name : "未分组" }}
          </h4>
          <p class="pv-path">
            <i class="ri-folder-3-line"></i>{{ group ? `stickers/${groupPath}` : "stickers/（根目录）" }}
          </p>
        </div>
      </header>
      <div class="metrics">
        <div class="metric">
          <div class="k">便签</div>
          <div class="v">{{ groupSummary.total }}<small>张</small></div>
        </div>
        <div class="metric" :class="{ warn: groupSummary.todo > 0 }">
          <div class="k">待完成任务</div>
          <div class="v">{{ groupSummary.todo }}<small>项</small></div>
        </div>
        <div class="metric" :class="{ ok: groupSummary.done > 0 }">
          <div class="k">已完成任务</div>
          <div class="v">{{ groupSummary.done }}<small>项</small></div>
        </div>
      </div>
      <div class="pv-body">
        <div class="sect-title">最近更新</div>
        <p v-if="groupSummary.recent.length === 0" class="pv-empty-line">该文件夹下还没有便签。</p>
        <ul v-else class="recent">
          <li v-for="s in groupSummary.recent" :key="s.id">
            <span class="recent-title">{{ s.title || "（无标题）" }}</span>
            <span class="recent-time">{{ s.updated_at || "" }}</span>
          </li>
        </ul>
      </div>
    </template>

    <!-- 空态 -->
    <div v-else class="pv-blank">
      <i class="ri-file-search-line"></i>
      <p>点击左侧文件夹查看概览，或点击便签卡片查看内容与统计。</p>
      <p class="pv-blank-tip">把中间的分隔线拖到最右侧可以收起这个预览区。</p>
    </div>
  </aside>
</template>

<style scoped>
.preview {
  flex: 1;
  min-width: 0;
  overflow: auto;
  padding: 12px 16px 18px;
}
.pv-head {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  margin-bottom: 10px;
}
.pv-title-wrap {
  min-width: 0;
  flex: 1;
}
.pv-title {
  margin: 0;
  font-size: 16px;
  color: #2f3338;
  display: flex;
  align-items: center;
  gap: 6px;
}
.pv-path {
  margin: 3px 0 0;
  font-size: 11.5px;
  color: #9aa0a8;
  overflow-wrap: anywhere;
  display: flex;
  align-items: center;
  gap: 4px;
}
.pv-actions {
  display: flex;
  gap: 6px;
  flex: none;
}
.metrics {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(112px, 1fr));
  gap: 8px;
  margin: 10px 0 12px;
}
.metric {
  border: 1px solid rgba(0, 0, 0, 0.09);
  border-radius: 9px;
  padding: 7px 10px;
  background: #fbf7ec;
}
.metric .k {
  font-size: 11px;
  color: #9aa0a8;
}
.metric .v {
  font-size: 15px;
  font-weight: 600;
  color: #2f3338;
  margin-top: 2px;
}
.metric .v small {
  font-size: 11px;
  color: #9aa0a8;
  font-weight: 400;
  margin-left: 3px;
}
.metric.warn .v {
  color: #d33;
}
.metric.ok .v {
  color: #2e9e5b;
}
.pv-body {
  border-top: 1px solid rgba(0, 0, 0, 0.07);
  padding-top: 10px;
}
.sect-title {
  font-size: 11.5px;
  letter-spacing: 0.4px;
  color: #9aa0a8;
  margin: 8px 0 6px;
}
.md-body {
  font-size: 13px;
  line-height: 1.7;
  color: #333;
  overflow-wrap: anywhere;
}
.md-body :deep(h1),
.md-body :deep(h2),
.md-body :deep(h3) {
  font-size: 14.5px;
  margin: 10px 0 6px;
}
.md-body :deep(table) {
  border-collapse: collapse;
  font-size: 12.5px;
}
.md-body :deep(th),
.md-body :deep(td) {
  border: 1px solid rgba(0, 0, 0, 0.12);
  padding: 3px 8px;
}
.md-body :deep(pre) {
  background: #1f2430;
  color: #d8dee9;
  border-radius: 8px;
  padding: 9px 11px;
  overflow: auto;
}
.md-body :deep(img) {
  max-width: 100%;
}
.kv {
  display: grid;
  grid-template-columns: 96px 1fr;
  gap: 4px 10px;
  font-size: 12.5px;
  margin: 0;
}
.kv dt {
  color: #9aa0a8;
}
.kv dd {
  margin: 0;
  color: #2f3338;
  overflow-wrap: anywhere;
}
.kv code {
  background: rgba(0, 0, 0, 0.05);
  border-radius: 4px;
  padding: 0 4px;
}
.recent {
  list-style: none;
  margin: 0;
  padding: 0;
  display: grid;
  gap: 5px;
}
.recent li {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12.5px;
  padding: 5px 8px;
  border-radius: 7px;
  background: rgba(0, 0, 0, 0.02);
}
.recent-title {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: #2f3338;
}
.recent-time {
  flex: none;
  font-size: 11px;
  color: #9aa0a8;
}
.pv-empty-line {
  margin: 0;
  font-size: 12.5px;
  color: #9aa0a8;
}
.pv-blank {
  height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 6px;
  color: #b9b2a2;
  text-align: center;
}
.pv-blank i {
  font-size: 26px;
  color: #d6cfbd;
}
.pv-blank p {
  margin: 0;
  font-size: 12.5px;
  max-width: 260px;
  line-height: 1.7;
}
.pv-blank-tip {
  color: #cbc4b3;
}
.btn.small {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  border: 1px solid rgba(0, 0, 0, 0.12);
  border-radius: 8px;
  padding: 5px 11px;
  font-size: 12.5px;
  background: #fff;
  color: #333;
  cursor: pointer;
}
.btn.small:hover {
  background: #f2f4f7;
}
.btn.small.danger:hover {
  background: #ffe3e3;
  color: #d33;
}
</style>
