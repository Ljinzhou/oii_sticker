//! 编辑层退格（Backspace）智能行为（类似 Obsidian）：
//! 空列表项（标记后无内容）且光标在标记之后 → 删除本行的缩进与标记，
//! 清空该行退出列表；其余情况返回 `None` 交给默认退格。
//! 围栏代码块内同样交给默认退格。

use super::edit_indent::in_fence_at;
use super::list::{line_right_edge, list_marker_of};

/// 编辑模式下按退格（Backspace）的智能行为（类似 Obsidian）：
/// 当**空列表项**（标记后无任何内容）且光标位于标记之后时，删除本行的
/// 缩进与标记（清空该行，退出列表）。其余情况返回 `None` 交给默认退格。
pub fn handle_backspace_at_cursor(text: &str, cursor: usize) -> Option<(String, usize)> {
    let cursor = cursor.min(text.len());
    let line_start = text[..cursor].rfind('\n').map(|i| i + 1).unwrap_or(0);
    // 围栏代码块内：交给默认退格。
    if in_fence_at(text, line_start) {
        return None;
    }
    let line_end = line_right_edge(text, cursor, line_start);
    let line = &text[line_start..line_end];

    let leading = line.len() - line.trim_start().len();
    let trimmed = &line[leading..];
    let marker = list_marker_of(trimmed)?;

    let marker_len = marker.len();
    let rest = trimmed.get(marker_len..).unwrap_or("");
    let in_line = cursor - line_start;

    // 空列表项（标记后无内容）且光标在标记之后 → 删除整行标记与缩进。
    if rest.trim().is_empty() && in_line >= leading + marker_len {
        let new_text = format!("{}{}", &text[..line_start], &text[line_end..]);
        return Some((new_text, line_start));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 空无序列表项退格：删除 `- `。
    #[test]
    fn backspace_removes_empty_bullet() {
        let text = "- ";
        let (new_text, new_cursor) = handle_backspace_at_cursor(text, text.len()).unwrap();
        assert_eq!(new_text, "");
        assert_eq!(new_cursor, 0);
    }

    /// 空任务列表项退格：删除 `- [ ] `。
    #[test]
    fn backspace_removes_empty_task() {
        let text = "- [ ] ";
        let (new_text, new_cursor) = handle_backspace_at_cursor(text, text.len()).unwrap();
        assert_eq!(new_text, "");
        assert_eq!(new_cursor, 0);
    }

    /// 空有序列表项退格：删除 `2. `。
    #[test]
    fn backspace_removes_empty_ordered() {
        let text = "- 文本\n2. ";
        let cursor = text.len();
        let (new_text, new_cursor) = handle_backspace_at_cursor(text, cursor).unwrap();
        assert_eq!(new_text, "- 文本\n");
        assert_eq!(new_cursor, "- 文本\n".len());
    }

    /// 有内容的列表行退格：交给默认行为（返回 None）。
    #[test]
    fn backspace_content_list_returns_none() {
        assert!(handle_backspace_at_cursor("- 文本", 8).is_none());
        assert!(handle_backspace_at_cursor("2. 文本", 9).is_none());
    }

    /// 光标在标记内部退格：交给默认行为（返回 None）。
    #[test]
    fn backspace_inside_marker_returns_none() {
        assert!(handle_backspace_at_cursor("- ", 1).is_none(), "光标在 `-` 之后");
        assert!(handle_backspace_at_cursor("2. ", 2).is_none(), "光标在 `2.` 中间");
    }

    /// 围栏代码块内退格：交给默认行为。
    #[test]
    fn backspace_inside_fence_noop() {
        let text = "```\n  - x\n```";
        let cursor = text.find("  - x").unwrap() + 4;
        assert!(handle_backspace_at_cursor(text, cursor).is_none());
    }
}
