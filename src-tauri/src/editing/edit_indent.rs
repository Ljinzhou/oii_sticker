//! 编辑层智能缩进：Tab / Shift+Tab（类似 Notion / Obsidian）。
//! - Tab：行首生成嵌套列表标记；有序行内按「父链 + 子序号」生成复合编号
//!   （`1. 文本\n2. ` 按 Tab → `1. 文本\n  1.1 `）；
//! - Shift+Tab：有序编号提升一级（目标层编号 = 该层已有项数 + 1），
//!   无序 / 任务列表去掉前 2 空格缩进；
//! - 围栏代码块（` ``` ` / ` ~~~ `）内一律退化为普通缩进 / 默认行为，
//!   由 `in_fence_at` 判定（CommonMark 语义：` ``` ` 只能用 ` ``` ` 关闭）。

use super::list::{
    format_chain, line_right_edge, list_marker_of, nested_marker_of, ordered_chain_of,
};

/// 编辑模式下按 Tab 的智能缩进，返回 `(新文本, 新光标字节偏移)`：
/// - 光标在**行首/空行**：继承上一行的列表类型，生成嵌套子项标记
///   （`  - `、`  - [ ] `、`  1. `）；上一行不是列表则插入 2 空格。
/// - 光标在**有序列表行内**：缩进一层并生成复合编号 `父级.子序号`
///   （`1. 文本\n2. ` 中在 `2. ` 按 Tab → `1. 文本\n  1.1 `）。
/// - 光标在其他列表行内：在行首插入 2 空格，把该行缩进一层。
/// - 光标在普通行内：在行首插入 2 空格。
///
/// 注：`1.1` 是**编辑层**的复合编号标记，pulldown-cmark 不识别（渲染中
/// 普通段落落文本 `1.1 ...`）；如需渲染端也显示多级编号需另行扩展解析器。
pub fn handle_tab_at_cursor(text: &str, cursor: usize) -> (String, usize) {
    let cursor = cursor.min(text.len());
    let line_start = text[..cursor].rfind('\n').map(|i| i + 1).unwrap_or(0);

    // 围栏代码块内：Tab 只做普通缩进（行首插 2 空格），不做列表操作。
    if in_fence_at(text, line_start) {
        let new_text = format!("{}{}{}", &text[..line_start], "  ", &text[line_start..]);
        return (new_text, cursor + 2);
    }

    let prefix = &text[line_start..cursor];

    if prefix.trim().is_empty() {
        // 光标在行首（或其后全是空白）：生成嵌套列表项。
        let marker = if line_start > 0 {
            let prev_start = text[..line_start - 1].rfind('\n').map(|i| i + 1).unwrap_or(0);
            nested_marker_of(&text[prev_start..line_start - 1])
        } else {
            None
        };
        let indent = match marker {
            Some(m) => format!("  {m}"),
            None => "  ".to_string(),
        };
        let new_text = format!("{}{}{}", &text[..line_start], indent, &text[line_start..]);
        (new_text, line_start + indent.len())
    } else {
        // 光标在行内。
        let line_end = line_right_edge(text, cursor, line_start);
        let line = &text[line_start..line_end];
        let leading = line.len() - line.trim_start().len();
        let trimmed = &line[leading..];

        if ordered_chain_of(trimmed).is_some() {
            // 有序列表：缩进一层（降级）。新编号规则（Notion 式）：
            // - 上一有序行链 P 是当前链 C 的真前缀（C 是 P 的深层）→ 新链 = C + [1]
            // - 否则（P 与 C 同级或更浅）→ 新链 = P + [1]（成为 P 的子项）
            // 缩进 = (链长-1) × 2 空格。
            let cur_chain = ordered_chain_of(trimmed).unwrap_or_default();
            let marker_len = list_marker_of(trimmed).map(|m| m.len()).unwrap_or(0);
            let new_chain = match find_prev_ordered_chain(text, line_start) {
                Some(parent) if is_prefix_of(&parent, &cur_chain) => {
                    let mut c = cur_chain.clone();
                    c.push(1);
                    c
                }
                Some(parent) => {
                    let mut c = parent.clone();
                    c.push(1);
                    c
                }
                // 前面没有可缩进到的有序项（如首个列表行）：Tab 无效果，原样返回
                // （不能重写标记，否则会丢掉行首缩进）。
                None => return (text.to_string(), cursor),
            };
            let indent = "  ".repeat(new_chain.len().saturating_sub(1));
            let new_marker = format!("{indent}{}", format_chain(&new_chain));
            let content_offset = cursor.saturating_sub(line_start + leading + marker_len);
            let new_text = format!(
                "{}{}{}",
                &text[..line_start],
                new_marker,
                &text[line_start + leading + marker_len..]
            );
            let new_cursor = line_start + new_marker.len() + content_offset;
            (new_text, new_cursor)
        } else {
            // 普通 / 无序 / 任务行：在行首插入 2 空格缩进（嵌套一层）。
            let new_text = format!("{}{}{}", &text[..line_start], "  ", &text[line_start..]);
            (new_text, cursor + 2)
        }
    }
}

/// 编辑模式下按 Shift+Tab 的智能行为（升级 / 取消缩进，类似 Notion）：
///
/// - **有序列表**：提升一级。目标层编号 = 该层（前缀匹配）已有项数 + 1
///   （`1. 文本\n  1.1 文本` 中 `1.1` 升级 → `2. 文本`；
///   `1.2.1` 升级 → `1.3 `）。顶层（链长 1）无效果。
/// - **无序 / 任务列表**：去掉前 2 空格缩进；已是顶层则无效果。
/// - 普通行：无效果。
pub fn handle_shift_tab_at_cursor(text: &str, cursor: usize) -> Option<(String, usize)> {
    let cursor = cursor.min(text.len());
    let line_start = text[..cursor].rfind('\n').map(|i| i + 1).unwrap_or(0);
    // 围栏代码块内：交给默认行为。
    if in_fence_at(text, line_start) {
        return None;
    }
    let line_end = line_right_edge(text, cursor, line_start);
    let line = &text[line_start..line_end];
    let leading = line.len() - line.trim_start().len();
    let trimmed = &line[leading..];

    // 有序列表：提升一级。
    if let Some(chain) = ordered_chain_of(trimmed) {
        if chain.len() == 1 {
            return None; // 已是顶层
        }
        let marker_len = list_marker_of(trimmed).map(|m| m.len()).unwrap_or(0);
        let target_len = chain.len() - 1;
        let prefix = if target_len >= 2 {
            &chain[..target_len - 1]
        } else {
            &[][..]
        };
        let mut new_chain = prefix.to_vec();
        new_chain.push((count_chain_rows(text, prefix, target_len) + 1) as u32);
        let indent = "  ".repeat(new_chain.len().saturating_sub(1));
        let new_marker = format!("{indent}{}", format_chain(&new_chain));
        let content_offset = cursor.saturating_sub(line_start + leading + marker_len);
        let new_text = format!(
            "{}{}{}",
            &text[..line_start],
            new_marker,
            &text[line_start + leading + marker_len..]
        );
        let new_cursor = line_start + new_marker.len() + content_offset;
        return Some((new_text, new_cursor));
    }

    // 无序 / 任务列表：去掉前 2 空格缩进。
    if leading >= 2 && list_marker_of(trimmed).is_some() {
        let new_text = format!("{}{}", &text[..line_start], &text[line_start + 2..]);
        let new_cursor = cursor.saturating_sub(2);
        return Some((new_text, new_cursor));
    }

    None
}

/// 判断 `line_start` 所在行是否在围栏代码块（` ``` ` 或 ` ~~~ `）内。
/// 按 CommonMark 语义跟踪**当前围栏类型**：` ``` ` 只能用 ` ``` ` 关闭，` ~~~ ` 只能用
/// ` ~~~ ` 关闭（混合计数会误判），围栏内与围栏类型同字符开头的行视为关闭。
pub(super) fn in_fence_at(text: &str, line_start: usize) -> bool {
    let mut fence: Option<char> = None; // None = 不在围栏内；Some(c) = 围栏字符
    for line in text[..line_start].lines() {
        let t = line.trim_start();
        match fence {
            None => {
                if t.starts_with("```") {
                    fence = Some('`');
                } else if t.starts_with("~~~") {
                    fence = Some('~');
                }
            }
            Some(c) => {
                if (c == '`' && t.starts_with("```")) || (c == '~' && t.starts_with("~~~")) {
                    fence = None;
                }
            }
        }
    }
    fence.is_some()
}

/// 从当前行往前找**最近的有序列表项**，返回其编号链（如 `1. 文本` → `[1]`）。
/// 跳过空行；遇到非空非有序行（普通文本 / 无序列表）则停止并返回 None。
fn find_prev_ordered_chain(text: &str, line_start: usize) -> Option<Vec<u32>> {
    let mut search_start = line_start;
    while search_start > 0 {
        let prev_start = text[..search_start - 1].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let prev = &text[prev_start..search_start - 1];
        let t = prev.trim_start();
        if let Some(chain) = ordered_chain_of(t) {
            return Some(chain);
        }
        if !t.is_empty() {
            return None;
        }
        search_start = prev_start;
    }
    None
}

/// `prefix` 是否为 `full` 的真前缀（prefix ⊂ full）。
fn is_prefix_of(prefix: &[u32], full: &[u32]) -> bool {
    prefix.len() < full.len() && full.starts_with(prefix)
}

/// 统计文本中「链长 == len 且以 prefix 为前缀」的有序列表行数。
/// 前缀为空表示统计该层的全部行（如升级到顶层时）。
fn count_chain_rows(text: &str, prefix: &[u32], len: usize) -> usize {
    let mut count = 0;
    for line in text.lines() {
        let t = line.trim_start();
        if let Some(chain) = ordered_chain_of(t) {
            if chain.len() == len && chain.starts_with(prefix) {
                count += 1;
            }
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Tab 智能缩进 ----

    /// 无序列表项内回车后按 Tab：新空行应出现嵌套子项 `  - `。
    #[test]
    fn tab_inherits_unordered_list() {
        let text = "- 这里是文本\n";
        let cursor = text.len(); // 光标在末尾（新空行行首）
        let (new_text, new_cursor) = handle_tab_at_cursor(text, cursor);
        assert_eq!(new_text, "- 这里是文本\n  - ");
        assert_eq!(new_cursor, new_text.len(), "光标应停在嵌套标记之后");
    }

    /// 任务列表继承：`  - [ ] `（子项未勾选）。
    #[test]
    fn tab_inherits_task_list() {
        let text = "- [ ] 买牛奶\n";
        let (new_text, new_cursor) = handle_tab_at_cursor(text, text.len());
        assert_eq!(new_text, "- [ ] 买牛奶\n  - [ ] ");
        assert_eq!(new_cursor, new_text.len());
    }

    /// 有序列表继承：嵌套从 1 重排。
    #[test]
    fn tab_inherits_ordered_list() {
        let text = "3. 第三项\n";
        let (new_text, _) = handle_tab_at_cursor(text, text.len());
        assert_eq!(new_text, "3. 第三项\n  1. ");
    }

    /// 上一行是普通文本：Tab 只插入 2 空格缩进。
    #[test]
    fn tab_on_plain_text_inserts_two_spaces() {
        let text = "普通段落\n";
        let (new_text, new_cursor) = handle_tab_at_cursor(text, text.len());
        assert_eq!(new_text, "普通段落\n  ");
        assert_eq!(new_cursor, new_text.len());
    }

    /// CJK 多字节内容下，字节偏移光标不会切碎字符。
    #[test]
    fn tab_cjk_byte_offsets() {
        let text = "- 第一层\n";
        let (new_text, new_cursor) = handle_tab_at_cursor(text, text.len());
        assert_eq!(new_text, "- 第一层\n  - ");
        assert_eq!(new_cursor, new_text.len());

        // 光标停在 CJK 字符边界（"普通段" 之后）时安全缩进。
        let text2 = "普通段落内容";
        let cursor = "普通段".len();
        let (nt, nc) = handle_tab_at_cursor(text2, cursor);
        assert_eq!(nt, "  普通段落内容");
        assert_eq!(nc, cursor + 2);
    }

    /// 畸形行 `- [ ]x`（任务标记后无空白）不应生成 `- [ ] ` 子项；
    /// 它仍是无序列表项（pulldown 同样按 `- ` 识别），嵌套用 `- `。
    #[test]
    fn tab_ignores_malformed_task_marker() {
        let text = "- [ ]x 不是任务\n";
        let (new_text, _) = handle_tab_at_cursor(text, text.len());
        assert!(new_text.contains("- [ ]x 不是任务\n  - "), "got: {new_text}");
        assert!(
            !new_text.contains("- [ ] "),
            "畸形任务标记不应生成 `- [ ] ` 子项，got: {new_text}"
        );
    }

    /// 光标在列表行中间按 Tab：整行缩进一层（行首插入 2 空格）。
    #[test]
    fn tab_indents_current_list_line() {
        let text = "- 第一层";
        let cursor = 2; // 光标在 `- ` 后
        let (new_text, new_cursor) = handle_tab_at_cursor(text, cursor);
        assert_eq!(new_text, "  - 第一层");
        assert_eq!(new_cursor, cursor + 2);
    }

    /// 连续 Tab 继续缩进（对已有子项再深一层）。
    #[test]
    fn tab_repeated_nests_deeper() {
        let text = "- 第一层\n  - ";
        let cursor = text.len();
        let (new_text, _) = handle_tab_at_cursor(text, cursor);
        assert_eq!(new_text, "- 第一层\n    - ");
    }

    /// 有序列表行内 Tab：生成复合编号。
    #[test]
    fn tab_ordered_creates_compound() {
        let text = "1. 文本\n2. ";
        let cursor = text.len();
        let (new_text, new_cursor) = handle_tab_at_cursor(text, cursor);
        assert_eq!(new_text, "1. 文本\n  1.1 ");
        assert_eq!(new_cursor, new_text.len());
    }

    // ---- Shift+Tab ----

    /// 有序列表提升一级：`  1.1 文本` → `2. 文本`。
    #[test]
    fn shift_tab_promotes_ordered() {
        let text = "1. 文本\n  1.1 文本";
        let cursor = text.len();
        let (new_text, _) = handle_shift_tab_at_cursor(text, cursor).unwrap();
        assert_eq!(new_text, "1. 文本\n2. 文本");
    }

    /// 无序列表取消缩进：去掉前 2 空格。
    #[test]
    fn shift_tab_dedents_unordered() {
        let text = "  - 子项";
        let cursor = text.len();
        let (new_text, new_cursor) = handle_shift_tab_at_cursor(text, cursor).unwrap();
        assert_eq!(new_text, "- 子项");
        assert_eq!(new_cursor, cursor - 2);
    }

    /// 顶层无序列表 Shift+Tab 无效果。
    #[test]
    fn shift_tab_top_level_noop() {
        assert!(handle_shift_tab_at_cursor("- 顶层", "- 顶层".len()).is_none());
    }

    // ---- 围栏 ----

    /// 围栏代码块内 Tab 只做普通缩进。
    #[test]
    fn tab_inside_fence_plain_indent() {
        let text = "```\n- [ ] 代码示例\n```";
        let line_start = text.find("```\n").unwrap() + 4;
        let (new_text, _) = handle_tab_at_cursor(text, line_start);
        assert_eq!(new_text, "```\n  - [ ] 代码示例\n```");
    }

    /// 混合围栏类型：``` 只能被 ``` 关闭。
    #[test]
    fn fence_type_mixing() {
        let text = "```\n~~~\n```\n";
        // 第二行 ~~~ 在 ``` 围栏内（类型不同不关闭）
        assert!(in_fence_at(text, text.find("~~~").unwrap()));
        // 第三行 ``` 是关闭行：该行自身仍属于围栏（关闭行属于围栏）
        assert!(in_fence_at(text, text.rfind("```").unwrap()));
        // 围栏之后的行不在围栏内
        assert!(!in_fence_at(text, text.len()));
    }
}
