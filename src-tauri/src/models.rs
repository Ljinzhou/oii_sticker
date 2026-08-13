//! 全局数据模型定义。
//!
//! 所有结构体与 SQLite 表 1:1 对应（数据结构对齐旧项目 `oi_sticker`，
//! 保证旧 `stickers.db` 兼容）；相比源项目补充了 serde 派生，供 Tauri
//! 命令与前端 `invoke` 传参/返回使用。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 单条便签主表记录，对应 SQLite `stickers` 表。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sticker {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub title: String,
    pub content: String,
    /// h1-h6 转化为子便签时记录的原始 heading level；0 表示普通根便签。
    pub heading_level: i32,
    pub pos_x: i32,
    pub pos_y: i32,
    pub width: i32,
    pub height: i32,
    /// 0.0 ~ 1.0，控制背景透明度。
    pub opacity: f32,
    /// hex 颜色，例如 "#FF5733"（源项目为 hex RGBA，兼容读取）
    pub bg_color: Option<String>,
    pub always_on_top: bool,
    pub auto_scroll: bool,
    pub is_completed: bool,
    /// 运行时标记，提醒触发后由 scheduler 写入。
    pub alert_active: bool,
    /// 窗口模式字符串（"display"/"interact"/"edit"），落库持久化；
    /// 类型安全访问用 [`Sticker::mode`]。
    pub display_mode: String,
    pub created_at: String,
    pub updated_at: String,
}

/// 便签窗口模式（对应 DB `display_mode` 列）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StickerMode {
    /// 展示模式：只读、低透明度（"收起来"状态）。
    Display,
    /// 交互模式：可点击待办、展开子便签。
    Interact,
    /// 编辑模式：编辑便签。
    Edit,
}

impl StickerMode {
    pub fn as_str(self) -> &'static str {
        match self {
            StickerMode::Display => "display",
            StickerMode::Interact => "interact",
            StickerMode::Edit => "edit",
        }
    }

    /// 解析 DB / UI 字符串；未知值回退为 Display（与 schema 默认一致）。
    pub fn parse(s: &str) -> StickerMode {
        match s {
            "interact" => StickerMode::Interact,
            "edit" => StickerMode::Edit,
            _ => StickerMode::Display,
        }
    }
}

impl Serialize for StickerMode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for StickerMode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(StickerMode::parse(&s))
    }
}

impl Sticker {
    /// display_mode 的类型安全访问器（未知值回退 Display）。
    pub fn mode(&self) -> StickerMode {
        StickerMode::parse(&self.display_mode)
    }
}

/// 便签附加属性，对应 SQLite `sticker_attrs` 表（提醒数据）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StickerAttrs {
    pub sticker_id: i64,
    /// 截止日期 ISO 8601
    pub due_date: Option<String>,
    /// 下次提醒时间
    pub remind_at: Option<String>,
    /// 提醒规则字符串，例如 "1d"、"12h"、"weekly"
    pub remind_rule: Option<String>,
    pub is_recurring: bool,
}

/// 便签窗口个性化偏好，对应 SQLite `sticker_prefs` 表。
///
/// 该表是可选的（与 stickers 1:1）。未填写任何字段时表示
/// "沿用 system_config 中的默认便签偏好"。各字段都允许 None（沿用默认值）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StickerPrefs {
    pub sticker_id: i64,
    /// 背景透明度 0.0 ~ 1.0
    pub opacity: Option<f32>,
    /// 标题是否居中（默认沿用 system_config，默认值为 false / 居左）
    pub title_centered: Option<bool>,
    /// 标题字号（像素）
    pub title_font_size: Option<i32>,
    /// 正文字号（像素）。Markdown 标签的字号将按此值同比例缩放。
    pub body_font_size: Option<i32>,
    /// 背景颜色：#RRGGBB 十六进制字符串
    pub bg_color: Option<String>,
    /// 正文颜色：#RRGGBB 十六进制字符串
    pub text_color: Option<String>,
    /// 自动滚动速度（px/s）。None 表示沿用 system_config 默认值。
    pub auto_scroll_speed: Option<i32>,
}

/// 经过默认值填充后真正驱动 UI 的"最终偏好"。所有字段都不是 None。
/// 调用方只读这份快照，避免每个像素都要回查 system_config。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectivePrefs {
    pub opacity: f32,
    pub title_centered: bool,
    pub title_font_size: i32,
    pub body_font_size: i32,
    pub bg_color: String,
    pub text_color: String,
    /// 自动滚动速度（px/s）。
    pub auto_scroll_speed: i32,
}

impl Default for EffectivePrefs {
    fn default() -> Self {
        Self {
            opacity: 0.9,
            title_centered: false,
            title_font_size: 14,
            body_font_size: 13,
            bg_color: "#CCFFCC".to_string(),
            text_color: "#222222".to_string(),
            auto_scroll_speed: 30,
        }
    }
}

/// 待办条目，对应 SQLite `todo_items` 表（渲染 `- [ ]` / `- [x]` 用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: i64,
    pub sticker_id: i64,
    pub child_sticker_id: Option<i64>,
    pub text: String,
    pub done: bool,
    pub completed_at: Option<String>,
    pub sort_order: i32,
    pub due_date: Option<String>,
    pub remind_at: Option<String>,
    pub remind_rule: Option<String>,
    pub is_recurring: bool,
}

/// `system_config` 表条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigEntry {
    pub key: String,
    pub value: String,
    pub description: String,
    pub updated_at: String,
}

/// 应用启动时一次性缓存的配置快照。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemConfig {
    pub entries: HashMap<String, String>,
}

impl SystemConfig {
    /// 读取字符串配置，缺失时返回 `default`。
    pub fn get_or(&self, key: &str, default: &str) -> String {
        self.entries
            .get(key)
            .cloned()
            .unwrap_or_else(|| default.to_string())
    }

    /// 读取 u32 配置，缺失或解析失败时返回 `default`。
    pub fn get_u32(&self, key: &str, default: u32) -> u32 {
        self.entries
            .get(key)
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(default)
    }

    /// 读取 f32 配置，缺失或解析失败时返回 `default`。
    pub fn get_f32(&self, key: &str, default: f32) -> f32 {
        self.entries
            .get(key)
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(default)
    }

    /// 读取 bool 配置："1"/"true"/"yes"/"on" 视为 true，其余 false。
    pub fn get_bool(&self, key: &str, default: bool) -> bool {
        match self.entries.get(key).map(|s| s.as_str()) {
            Some(v) => matches!(v.to_lowercase().as_str(), "1" | "true" | "yes" | "on"),
            None => default,
        }
    }

    /// 根据便签自己的 prefs 与 system_config 默认值合并，得到最终生效值。
    /// 字段优先级：prefs 中为 None → 沿用 system_config。
    ///
    /// `sticker_bg_color` 用于 bg_color 字段的兜底：
    /// 用户在 sticker 表里直接保存的 bg_color 优先级高于 system default。
    /// 这是为了避免打开设置弹层时把 `#CCFFCC` 这种 system default
    /// 覆盖掉用户实际保存在 sticker.bg_color 中的自定义颜色。
    pub fn effective(&self, p: &StickerPrefs, sticker_bg_color: Option<&str>) -> EffectivePrefs {
        EffectivePrefs {
            opacity: p
                .opacity
                .unwrap_or_else(|| self.get_f32("default_sticker_opacity", 0.9)),
            title_centered: p.title_centered.unwrap_or_else(|| {
                self.get_bool("default_sticker_title_centered", false)
            }),
            title_font_size: p.title_font_size.unwrap_or_else(|| {
                self.get_u32("default_sticker_title_font_size", 14) as i32
            }),
            body_font_size: p.body_font_size.unwrap_or_else(|| {
                self.get_u32("default_sticker_body_font_size", 13) as i32
            }),
            bg_color: p
                .bg_color
                .clone()
                .or_else(|| sticker_bg_color.map(|s| s.to_string()))
                .unwrap_or_else(|| self.get_or("default_sticker_bg_color", "#CCFFCC")),
            text_color: p
                .text_color
                .clone()
                .unwrap_or_else(|| self.get_or("default_sticker_text_color", "#222222")),
            auto_scroll_speed: p.auto_scroll_speed.unwrap_or_else(|| {
                self.get_u32("default_sticker_auto_scroll_speed", 30) as i32
            }),
        }
    }
}

/// 图片资源，对应 SQLite `assets` 表（源项目缺失 repo，本工程补齐）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    pub id: i64,
    pub sticker_id: Option<i64>,
    pub name: String,
    pub mime_type: String,
    pub file_path: String,
    pub file_size: i64,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sticker_mode_roundtrip() {
        assert_eq!(StickerMode::parse("display"), StickerMode::Display);
        assert_eq!(StickerMode::parse("interact"), StickerMode::Interact);
        assert_eq!(StickerMode::parse("edit"), StickerMode::Edit);
        // 未知值回退 display。
        assert_eq!(StickerMode::parse(""), StickerMode::Display);
        assert_eq!(StickerMode::parse("EDIT"), StickerMode::Display); // 大小写敏感
        // as_str 往返。
        assert_eq!(StickerMode::Display.as_str(), "display");
        assert_eq!(StickerMode::Interact.as_str(), "interact");
        assert_eq!(StickerMode::Edit.as_str(), "edit");
    }

    #[test]
    fn sticker_mode_serde_roundtrip() {
        assert_eq!(
            serde_json::to_string(&StickerMode::Interact).unwrap(),
            "\"interact\""
        );
        assert_eq!(
            serde_json::from_str::<StickerMode>("\"edit\"").unwrap(),
            StickerMode::Edit
        );
        // 未知字符串回退 Display（与 parse 语义一致）
        assert_eq!(
            serde_json::from_str::<StickerMode>("\"bogus\"").unwrap(),
            StickerMode::Display
        );
    }

    #[test]
    fn effective_prefs_merge_priority() {
        let mut cfg = SystemConfig::default();
        cfg.entries.insert("default_sticker_opacity".into(), "0.5".into());
        cfg.entries.insert("default_sticker_bg_color".into(), "#CCFFCC".into());
        cfg.entries.insert("default_sticker_text_color".into(), "#222222".into());
        cfg.entries.insert("default_sticker_title_font_size".into(), "14".into());

        // 空 prefs：全部走 system_config / 兜底
        let empty = StickerPrefs::default();
        let e = cfg.effective(&empty, None);
        assert_eq!(e.opacity, 0.5);
        assert_eq!(e.bg_color, "#CCFFCC");
        assert_eq!(e.title_font_size, 14);

        // prefs 覆盖 system_config
        let mut p = StickerPrefs::default();
        p.opacity = Some(0.95);
        let e = cfg.effective(&p, None);
        assert_eq!(e.opacity, 0.95);

        // sticker.bg_color 兜底优先级高于 system default
        let e = cfg.effective(&empty, Some("#123456"));
        assert_eq!(e.bg_color, "#123456");

        // prefs.bg_color 最高
        let mut p = StickerPrefs::default();
        p.bg_color = Some("#ABCDEF".into());
        let e = cfg.effective(&p, Some("#123456"));
        assert_eq!(e.bg_color, "#ABCDEF");
    }
}
