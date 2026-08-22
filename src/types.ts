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
  alert_active: boolean;
  display_mode: string;
  created_at: string;
  updated_at: string;
}

export interface NewSticker {
  parent_id?: number | null;
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
  alert_active?: boolean | null;
  display_mode?: string | null;
}

export interface StickerAttrs {
  sticker_id: number;
  due_date: string | null;
  remind_at: string | null;
  remind_rule: string | null;
  is_recurring: boolean;
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

export interface StickerGroup {
  id: number;
  name: string;
  sort_order: number;
  created_at: string;
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
  sort_order?: number;
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
