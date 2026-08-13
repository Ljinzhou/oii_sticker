//! 编辑层回车（Enter）智能行为（类似 Obsidian 列表逻辑）。
//!
//! 在列表行回车时自动续行：非空列表项 → 新行带相同缩进与标记（有序
//! 编号递增，任务统一为未勾选）；空列表项 → 删除本行缩进与标记退出列表；
//! 光标在标记之前 → 在行上方插入一个空列表项。围栏代码块内交给默认回车。

use super::edit_indent::in_fence_at;
use super::list::{continuation_marker, line_right_edge};

/// 编辑模式下按回车（Enter/Return）的智能行为（类似 Obsidian 列表逻辑），
/// 返回 `(新文本, 新光标字节偏移)`；不是列表行时返回 `None`（交给默认回车）：
///
/// - 列表项**有内容**：光标处断行，新行带相同缩进与标记
///   （`- 文本` → `- 文本\n- `，`1. 项` → `1. 项\n2. `（编号递增），
///   `- [ ] 买` → `- [ ] 买\n- [ ] `）。
///   光标在标记之前（行首）时，在行上方插入一个空列表项。
/// - 列表项**内容为空**：删除本行的缩进与标记（退出列表，行变空）；
///   再按一次回车才是真正的换行。
pub fn handle_enter_at_cursor(text: &str, cursor: usize) -> Option<(String, usize)> {
    let cursor = cursor.min(text.len());
    let line_start = text[..cursor].rfind('\n').map(|i| i + 1).unwrap_or(0);
    // 围栏代码块内：默认换行（不插入列表标记，避免改写代码内容）。
    if in_fence_at(text, line_start) {
        return None;
    }
    let line_end = line_right_edge(text, cursor, line_start);
    let line = &text[line_start..line_end];

    let leading = line.len() - line.trim_start().len();
    let trimmed = &line[leading..];
    let marker = continuation_marker(trimmed)?;

    let marker_len = marker.len();
    let in_line = cursor - line_start;

    // 标记后的内容：`get` 防止标记长于行本身的情况（如 `- [ ]` 无尾随空格）。
    let rest = trimmed.get(marker_len..).unwrap_or("");
    if rest.trim().is_empty() {
        // 空列表项：删除缩进与标记，退出列表。
        let new_text = format!("{}{}", &text[..line_start], &text[line_end..]);
        return Some((new_text, line_start));
    }

    let indent = &line[..leading];
    if in_line <= leading {
        // 光标在标记之前（行首）：在行上方插入一个空列表项。
        let new_text = format!(
            "{}{}{}\n{}",
            &text[..line_start],
            indent,
            marker,
            &text[line_start..]
        );
        let new_cursor = line_start + leading + marker_len;
        Some((new_text, new_cursor))
    } else {
        // 光标在标记后/内容中：光标处断行，新行带缩进 + 标记。
        let prefix = &text[..cursor];
        let suffix = &text[cursor..];
        let new_text = format!("{prefix}\n{indent}{marker}{suffix}");
        let new_cursor = prefix.len() + 1 + leading + marker_len;
        Some((new_text, new_cursor))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 回车续行：`1.1 文本` 回车 → 同级 `1.2 `（用户场景）。
    #[test]
    fn enter_compound_increments_sibling() {
        let text = "1. 文本\n  1.1 文本";
        let cursor = text.len();
        let (new_text, new_cursor) = handle_enter_at_cursor(text, cursor).unwrap();
        assert_eq!(new_text, "1. 文本\n  1.1 文本\n  1.2 ");
        assert_eq!(new_cursor, new_text.len());
    }

    /// 回车续行：深层复合编号 `1.2.1 文本` 回车 → 同级 `1.2.2 `。
    #[test]
    fn enter_compound_deep_increments() {
        let text = "1. 文本\n  1.1 文本\n  1.2 文本\n    1.2.1 文本";
        let cursor = text.len();
        let (new_text, _) = handle_enter_at_cursor(text, cursor).unwrap();
        assert_eq!(new_text, "1. 文本\n  1.1 文本\n  1.2 文本\n    1.2.1 文本\n    1.2.2 ");
    }

    /// 复合编号空项回车：退出列表（清空该行）。
    #[test]
    fn enter_compound_empty_exits() {
        let text = "1. 文本\n  1.1 ";
        let cursor = text.len();
        let (new_text, _) = handle_enter_at_cursor(text, cursor).unwrap();
        assert_eq!(new_text, "1. 文本\n");
    }

    /// 非空无序列表项回车：新行自动带 `- `。
    #[test]
    fn enter_continues_unordered_list() {
        let text = "- 文本";
        let cursor = text.len();
        let (new_text, new_cursor) = handle_enter_at_cursor(text, cursor).unwrap();
        assert_eq!(new_text, "- 文本\n- ");
        assert_eq!(new_cursor, new_text.len(), "光标停在标记之后");
    }

    /// 空无序列表项回车：删除 `- ` 退出列表。
    #[test]
    fn enter_exits_empty_list_item() {
        let text = "- ";
        let (new_text, new_cursor) = handle_enter_at_cursor(text, text.len()).unwrap();
        assert_eq!(new_text, "");
        assert_eq!(new_cursor, 0);
    }

    /// 有序列表：续行编号自动递增；空项回车退出列表。
    #[test]
    fn enter_handles_ordered_list() {
        let t1 = "3. 第三项";
        let (nt, nc) = handle_enter_at_cursor(t1, t1.len()).unwrap();
        assert_eq!(nt, "3. 第三项\n4. ", "续行编号应递增");
        assert_eq!(nc, nt.len());

        let (nt2, _) = handle_enter_at_cursor(&nt, nc).unwrap();
        assert_eq!(nt2, "3. 第三项\n", "空项 `4. ` 回车应清空该行");
    }

    /// 有序列表从 1 开始：`1. ` 回车 → `2. `。
    #[test]
    fn enter_ordered_increments_from_one() {
        let t1 = "1. 文本";
        let (nt, nc) = handle_enter_at_cursor(t1, t1.len()).unwrap();
        assert_eq!(nt, "1. 文本\n2. ");
        assert_eq!(nc, nt.len());
    }

    /// 嵌套有序列表：同缩进内递增。
    #[test]
    fn enter_ordered_nested_increments() {
        let t1 = "  1. 子项";
        let (nt, _) = handle_enter_at_cursor(t1, t1.len()).unwrap();
        assert_eq!(nt, "  1. 子项\n  2. ");
    }

    /// 任务列表：续行为未勾选 `- [ ] `；空任务项回车退出。
    #[test]
    fn enter_handles_task_list() {
        let t1 = "- [x] 已完成";
        let (nt, nc) = handle_enter_at_cursor(t1, t1.len()).unwrap();
        assert_eq!(nt, "- [x] 已完成\n- [ ] ");
        assert_eq!(nc, nt.len());

        let (nt2, _) = handle_enter_at_cursor(&nt, nc).unwrap();
        assert_eq!(nt2, "- [x] 已完成\n", "空 `- [ ] ` 回车应清空该行");
    }

    /// 任务标记无尾随空格（`- [ ]`，恰好 5 字节）回车：退出列表，不 panic。
    #[test]
    fn enter_handles_bare_task_marker() {
        let (nt, nc) = handle_enter_at_cursor("- [ ]", 5).unwrap();
        assert_eq!(nt, "");
        assert_eq!(nc, 0);
    }

    /// 普通行回车：交给默认行为（返回 None）。
    #[test]
    fn enter_plain_text_returns_none() {
        assert!(handle_enter_at_cursor("普通段落", 6).is_none());
        assert!(handle_enter_at_cursor("# 标题", 5).is_none());
    }

    /// 光标在列表行行首：在行上方插入一个空列表项。
    #[test]
    fn enter_at_line_start_inserts_above() {
        let text = "- 文本";
        let (new_text, new_cursor) = handle_enter_at_cursor(text, 0).unwrap();
        assert_eq!(new_text, "- \n- 文本");
        assert_eq!(new_cursor, 2, "光标停在新的空列表项标记后");
    }

    /// 嵌套列表：保留缩进。
    #[test]
    fn enter_keeps_indent() {
        let text = "  - 子项";
        let cursor = text.len();
        let (new_text, new_cursor) = handle_enter_at_cursor(text, cursor).unwrap();
        assert_eq!(new_text, "  - 子项\n  - ");
        assert_eq!(new_cursor, new_text.len());
    }

    // ---- 围栏代码块感知（Enter 侧）----

    /// 围栏代码块内回车：默认换行（不插入列表标记，避免改写代码）。
    #[test]
    fn enter_inside_fence_returns_none() {
        let text = "```\n- [ ] 代码示例\n```";
        let cursor = text.find("- [ ] 代码示例").unwrap() + "- [ ] 代码示例".len();
        assert!(handle_enter_at_cursor(text, cursor).is_none(), "代码块内不应续行");
        let t2 = "~~~\n1. 代码\n~~~";
        let c2 = t2.find("1. 代码").unwrap() + "1. 代码".len();
        assert!(handle_enter_at_cursor(t2, c2).is_none());
    }
}
