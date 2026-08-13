//! 斜杠菜单状态机（纯逻辑，UI 侧只做渲染与事件转发）。

use super::insert::extract_query;
use super::matcher::filter;
use super::SlashCommand;

/// 斜杠菜单的当前状态。
#[derive(Debug, Clone, Default)]
pub struct SlashMenuState {
    /// 菜单是否显示。
    pub visible: bool,
    /// 当前查询词（`/` 之后到光标前）。
    pub query: String,
    /// 过滤后的命令列表（按命令表顺序）。
    pub items: Vec<SlashCommand>,
    /// 当前选中项索引（循环移动）。
    pub selected: usize,
}

impl SlashMenuState {
    /// 根据文本与光标位置更新菜单状态。
    ///
    /// - 光标前存在斜杠查询 → 打开菜单并过滤命令；
    /// - 否则关闭菜单。
    pub fn update(&mut self, text: &str, cursor: usize, all: &[SlashCommand]) {
        match extract_query(text, cursor) {
            Some((query, _)) => {
                self.visible = true;
                self.query = query.clone();
                self.items = filter(all, &query)
                    .into_iter()
                    .cloned()
                    .collect();
                if self.items.is_empty() {
                    // 无匹配：关闭菜单（避免空菜单闪烁）。
                    self.visible = false;
                }
                self.selected = 0;
            }
            None => {
                self.visible = false;
                self.items.clear();
                self.query.clear();
                self.selected = 0;
            }
        }
    }

    /// 移动选中项（上下循环）。
    pub fn move_selection(&mut self, delta: isize) {
        if self.items.is_empty() {
            return;
        }
        let len = self.items.len() as isize;
        let cur = self.selected as isize;
        self.selected = ((cur + delta).rem_euclid(len)) as usize;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slash::commands::builtin_commands;

    fn state() -> SlashMenuState {
        SlashMenuState::default()
    }

    #[test]
    fn opens_on_slash() {
        let mut s = state();
        let all = builtin_commands();
        s.update("文本 /", 8, &all);
        assert!(s.visible);
        assert_eq!(s.items.len(), all.len(), "空查询显示全部命令");
        assert_eq!(s.selected, 0);
    }

    #[test]
    fn filters_by_query() {
        let mut s = state();
        let all = builtin_commands();
        s.update("/bt", 3, &all);
        assert!(s.visible);
        assert!(!s.items.is_empty());
        assert!(s
            .items
            .iter()
            .all(|c| c.pinyin.contains("biao") || c.alias.contains("标题") || c.name.contains("标")));
        // 无匹配时关闭。
        s.update("/zzzz", 5, &all);
        assert!(!s.visible);
    }

    #[test]
    fn closes_without_slash() {
        let mut s = state();
        let all = builtin_commands();
        s.update("/bt", 3, &all);
        assert!(s.visible);
        s.update("普通文本", 12, &all);
        assert!(!s.visible);
        assert!(s.items.is_empty());
    }

    #[test]
    fn selection_wraps() {
        let mut s = state();
        let all = builtin_commands();
        s.update("/", 1, &all);
        let len = s.items.len();
        // 上移：从 0 到末位。
        s.move_selection(-1);
        assert_eq!(s.selected, len - 1);
        // 下移：回到 0。
        s.move_selection(1);
        assert_eq!(s.selected, 0);
        // 大步移动循环。
        s.move_selection(len as isize + 1);
        assert_eq!(s.selected, 1);
    }
}
