//! 斜杠命令的插入逻辑（纯函数，可单测）。
//!
//! 规则：光标前紧邻 `/`（且 `/` 前是行首或空白）时，把 `/词` 视为斜杠
//! 查询；选中命令后整段替换为命令模板，光标落在模板末尾。

use super::SlashCommand;

/// 插入结果：新文本 + 新光标字节偏移。
#[derive(Debug, Clone, PartialEq)]
pub struct InsertResult {
    pub new_text: String,
    pub new_cursor: usize,
}

/// 提取光标前的斜杠查询。
///
/// 返回 `(query, query 起点字节偏移)`；以下情况返回 `None`：
/// - 光标前没有 `/`；
/// - `/` 前不是行首 / 空白（如 `a/b` 中的 `/` 不弹菜单）；
/// - query 跨行（含换行）。
pub fn extract_query(text: &str, cursor: usize) -> Option<(String, usize)> {
    let cursor = cursor.min(text.len());
    let before = &text[..cursor];
    let slash = before.rfind('/')?;
    // `/` 前必须是行首或空白。
    let before_slash = &before[..slash];
    if let Some(ch) = before_slash.chars().next_back() {
        if !ch.is_whitespace() {
            return None;
        }
    }
    let query = &before[slash + 1..];
    if query.contains('\n') {
        return None;
    }
    Some((query.to_string(), slash))
}

/// 把光标前的 `/query` 替换为命令模板。
///
/// 命令的 `insert` 回调返回 `None` 时表示该命令不接受当前输入（返回 None）。
pub fn apply(text: &str, cursor: usize, cmd: &SlashCommand) -> Option<InsertResult> {
    let (query, start) = extract_query(text, cursor)?;
    let template = (cmd.insert)(&query)?;
    let new_text = format!("{}{}{}", &text[..start], template, &text[cursor..]);
    let new_cursor = start + template.len();
    Some(InsertResult { new_text, new_cursor })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slash::commands::builtin_commands;

    fn cmd(id: &str) -> SlashCommand {
        builtin_commands()
            .into_iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| panic!("命令不存在: {id}"))
    }

    // ---- extract_query ----

    #[test]
    fn extract_basic() {
        // "abc /bt" 光标末尾 → query "bt"，起点 4（`/` 的位置）。
        let (q, start) = extract_query("abc /bt", 7).unwrap();
        assert_eq!(q, "bt");
        assert_eq!(start, 4);
    }
    #[test]
    fn extract_slash_only() {
        let (q, start) = extract_query("/", 1).unwrap();
        assert_eq!(q, "");
        assert_eq!(start, 0);
    }

    #[test]
    fn extract_rejects_non_boundary() {
        // `a/b` 的 `/` 前不是空白 → 不弹菜单。
        assert!(extract_query("a/b", 3).is_none());
        // 中文紧贴 `/` 前也不弹。
        assert!(extract_query("文本/bt", 6).is_none());
    }

    #[test]
    fn extract_rejects_multiline() {
        // query 含换行 → 不弹菜单。
        assert!(extract_query("/a\nb", 4).is_none());
        // 行首 `/`（前面是换行）可以弹菜单。
        let (q, start) = extract_query("a\n/bt", 5).unwrap();
        assert_eq!(q, "bt");
        assert_eq!(start, 2);
    }

    // ---- apply ----

    #[test]
    fn apply_replaces_query_with_template() {
        // `/bt` + 一级标题 → `/bt` 替换为 `# `。
        let r = apply("文本 /bt", 10, &cmd("heading1")).unwrap();
        assert_eq!(r.new_text, "文本 # ");
        assert_eq!(r.new_cursor, 7 + 2);
    }

    #[test]
    fn apply_keeps_suffix_after_cursor() {
        let r = apply("/bt 后面", 3, &cmd("heading1")).unwrap();
        assert_eq!(r.new_text, "#  后面");
        assert_eq!(r.new_cursor, 2);
    }

    #[test]
    fn apply_multi_line_template() {
        let r = apply("/code", 5, &cmd("code-block")).unwrap();
        assert_eq!(r.new_text, "```\n\n```");
        assert_eq!(r.new_cursor, r.new_text.len());
    }

    #[test]
    fn apply_requires_query_at_cursor() {
        assert!(apply("普通文本", 12, &cmd("heading1")).is_none());
    }
}
