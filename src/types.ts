// 前后端共享数据结构（与 src-tauri/src/models.rs 对应）

export type StickerMode = "display" | "interact" | "edit";

export interface Sticker {
  id: number;
  parent_id: number | null;
  group_id: number | null;
  title: string;
  content: string;
  heading_level: number;
  pos_x: number;
  pos_y: number;
  width: number;
  height: number;
  opacity: number;
  bg_color: string | null;
  always_on_top: boolean;
  auto_scroll: boolean;
  is_completed: boolean;
  display_mode: string;
  /** 8 位短随机 id（v19；仅程序内部使用：窗口标识 / assets 目录 / 合并工作空间去重）。 */
  uid?: string | null;
  /** 该便签 md 文件相对 stickers/ 的路径（v19；null = 按「分组路径/标题.md」派生）。 */
  file_name?: string | null;
  /** 上次退出时窗口是否隐藏（true=隐藏，启动不显示）；由后端维护。 */
  window_hidden?: boolean;
  created_at: string;
  updated_at: string;
}

export interface NewSticker {
  parent_id?: number | null;
  group_id?: number | null;
  title: string;
  content: string;
  heading_level?: number;
  pos_x: number;
  pos_y: number;
  width: number;
  height: number;
  opacity: number;
  bg_color?: string | null;
  always_on_top?: boolean;
  auto_scroll?: boolean;
}

export interface StickerPatch {
  title?: string | null;
  content?: string | null;
  pos_x?: number | null;
  pos_y?: number | null;
  width?: number | null;
  height?: number | null;
  opacity?: number | null;
  bg_color?: string | null;
  always_on_top?: boolean | null;
  auto_scroll?: boolean | null;
  is_completed?: boolean | null;
  display_mode?: string | null;
}

export interface StickerPrefs {
  sticker_id: number;
  opacity?: number | null;
  title_centered?: boolean | null;
  title_font_size?: number | null;
  body_font_size?: number | null;
  bg_color?: string | null;
  text_color?: string | null;
  auto_scroll_speed?: number | null;
}

export interface EffectivePrefs {
  opacity: number;
  title_centered: boolean;
  title_font_size: number;
  body_font_size: number;
  bg_color: string;
  text_color: string;
  auto_scroll_speed: number;
}

export interface SystemConfig {
  entries: Record<string, string>;
}

export interface WorkspaceEntry {
  id: string;
  name: string;
  path: string;
  created_at: string;
}

/** 备份文件信息（`workspace_inspect_backup_cmd` 返回值）。 */
export interface BackupInfo {
  format: number;
  has_manifest: boolean;
  name: string;
  workspace_id: string;
  created_at: string;
  /** 备份时刻（Unix 毫秒；旧版备份回退为 zip 文件的修改时间）。 */
  backup_at_ms: number;
  app_version: string;
  schema_version: number;
  entries: number;
  bytes: number;
  zip_bytes: number;
}

/** 恢复结果（`workspace_restore_cmd` 返回值）。 */
export interface RestoreOutcome {
  mode: string;
  name: string;
  root: string;
  entries: number;
  workspace: WorkspaceEntry | null;
  /** 覆盖模式的事前完整备份（可据此回退整个恢复）。 */
  rollback_zip: string | null;
  /** 覆盖模式旧数据的存放目录（未删除，确认无误后可手动清理）。 */
  rollback_dir: string | null;
}

export interface StickerGroup {
  id: number;
  name: string;
  sort_order: number;
  created_at: string;
  /** 父分组 id；null = 顶层（分组即文件夹，可无限嵌套）。 */
  parent_id?: number | null;
  /** 分组颜色（#RRGGBB）；null = 无颜色。 */
  color?: string | null;
}

export interface TodoBlock {
  id: string;
  sticker_id: number;
  title: string;
  block_title: string;
  description: string | null;
  is_completed: boolean;
  parent_id: string | null;
  reminder_at: string | null;
  due_at: string | null;
  repeat_rule: string | null;
  /** 提醒时间已触发（调度器写入；非空 = 高亮提示中）。 */
  reminded_at?: string | null;
  /** 截止提醒已触发（调度器写入）。 */
  due_notified_at?: string | null;
  /** 用户已确认提醒（非空 = 该时点不再高亮/复弹）。 */
  reminder_ack_at?: string | null;
  /** 用户已确认截止（非空 = 该时点不再高亮/复弹）。 */
  due_ack_at?: string | null;
  sort_order?: number;
  /** 完成时刻（本地时间 YYYY-MM-DD HH:MM:SS）；取消完成时清空。 */
  completed_at?: string | null;
  created_at: string;
  updated_at: string;
}

export interface TodoPatch {
  title?: string;
  block_title?: string;
  description?: string;
  is_completed?: boolean;
  reminder_at?: string;
  due_at?: string;
  repeat_rule?: string;
}

/** 跨便签 Todo 聚合查询过滤条件（与 Rust `TodoQueryFilter` 对应，全部可选）。 */
export interface TodoQueryFilter {
  completed?: boolean;
  due_before?: string;
  due_after?: string;
  remind_before?: string;
  remind_after?: string;
  keyword?: string;
}

/** 跨便签聚合返回项：TodoBlock 全字段 + 所属便签标题与所属 todo 块。
 *
 * 一个便签可含多个 todo 块（第 0 层容器），`owner_block_*` 供任务总览页
 * 按「便签 → 块 → 任务」三层区分展示；命名带 `owner_` 前缀以避开
 * TodoBlock 自身的 `block_title`（任务自身的卡头标题）。
 */
export type TodoBlockWithSticker = TodoBlock & {
  sticker_title: string;
  /** 所属 todo 块 id（第 0 层容器）。 */
  owner_block_id: string;
  /** 所属 todo 块标题（空串 = 未命名块）。 */
  owner_block_title: string;
};

export interface SlashItem {
  id: string;
  name: string;
  category: string;
  hint: string;
  template: string;
  pinyin?: string;
  alias?: string;
  isFunction?: boolean;
  iconId?: string;
}

export type SlashSection = "recent" | "function" | "markdown";

export interface SlashGroup {
  section: SlashSection;
  title: string;
  items: SlashItem[];
}
