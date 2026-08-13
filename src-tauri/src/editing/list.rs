//! 列表标记识别与格式化（编辑层共享工具）。
//!
//! 本模块只做「列表标记」的识别 / 格式化，不依赖任何其他模块，
//! 供编辑辅助模块（`edit_indent` / `edit_enter` / `edit_backspace`）共享：
//!
//! - 有序编号链：`1. ` → `[1]`，`1.1 ` → `[1,1]`（复合编号是编辑层标记，
//!   CommonMark 本身不识别）；
//! - 无序 / 任务 / 有序行首标记的统一识别（`list_marker_of` 等）；
//! - 行右边界计算（`line_right_edge`，CRLF 兼容）。

/// 解析有序编号链：`1. ` → `[1]`，`1.1 ` → `[1,1]`，`1.2.1 ` → `[1,2,1]`。
/// 规则：
/// - 单级（链长 1）为标准 markdown：数字 + `.`/`)` + 空格（`1. `、`12) `）；
/// - 多级（链长 ≥ 2）为编辑层复合编号：数字链 + 空格（`1.1 `），**无尾点**；
/// - `1.1. `（复合编号带尾点）不识别，按普通文本处理。
pub(super) fn ordered_chain_of(t: &str) -> Option<Vec<u32>> {
    let bytes = t.as_bytes();
    let mut i = 0;
    let mut chain = Vec::new();

    // 第一个数字段。
    let ds = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == ds {
        return None;
    }
    chain.push(t[ds..i].parse().ok()?);

    // 后续 `.数字` 段。
    while i < bytes.len() && bytes[i] == b'.' {
        let sep = i;
        i += 1;
        let ds = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == ds {
            // 尾点：`1.1. ` 之类，回退（多级不允许尾点）。
            i = sep;
            break;
        }
        chain.push(t[ds..i].parse().ok()?);
    }

    if chain.len() == 1 {
        // 标准单级标记：`.`/`)` + 空格。
        if bytes.get(i) == Some(&b'.') || bytes.get(i) == Some(&b')') {
            i += 1;
        } else {
            return None;
        }
        if bytes.get(i) == Some(&b' ') {
            Some(chain)
        } else {
            None
        }
    } else {
        // 复合编号：直接跟空格。
        if bytes.get(i) == Some(&b' ') {
            Some(chain)
        } else {
            None
        }
    }
}

/// 把编号链格式化为标记文本：链长 1 → `N. `；链长 ≥ 2 → `N.M.K `（无尾点）。
pub(super) fn format_chain(chain: &[u32]) -> String {
    let joined = chain
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(".");
    if chain.len() == 1 {
        format!("{joined}. ")
    } else {
        format!("{joined} ")
    }
}

/// 判断是否为有序列表标记（含多级复合编号）。
pub(super) fn is_ordered_marker(t: &str) -> bool {
    ordered_chain_of(t).is_some()
}

/// 识别行首列表标记，返回该行的标记文本（含尾随空格）：
/// 无序保留原符号（`- ` / `* ` / `+ `），有序保留原数字（`N. `），
/// 任务统一为未勾选 `- [ ] `。不是列表行返回 None。
pub(super) fn list_marker_of(t: &str) -> Option<String> {
    if (t.starts_with("- [ ]") || t.starts_with("- [x]") || t.starts_with("- [X]"))
        && marker_followed_by_ws(t, 5)
    {
        return Some("- [ ] ".to_string());
    }
    if t.starts_with("- ") {
        return Some("- ".to_string());
    }
    if t.starts_with("* ") {
        return Some("* ".to_string());
    }
    if t.starts_with("+ ") {
        return Some("+ ".to_string());
    }
    if let Some(chain) = ordered_chain_of(t) {
        return Some(format_chain(&chain));
    }
    None
}

/// 识别行首的列表标记，返回「嵌套一层后的标记」（不含缩进）：
/// `- [ ]` / `- [x]` / `- [X]` → `- [ ] `（子项未勾选）；
/// `- ` / `* ` / `+ ` → `- `；有序 `N. ` / `N) ` → `1. `（嵌套从 1 重排）。
pub(super) fn nested_marker_of(line: &str) -> Option<&'static str> {
    let t = line.trim_start();
    // 任务标记后必须跟空白或行尾，避免 `- [ ]x` 这种畸形写法被误判为任务项。
    // （`- ` 等无序标记已自带尾随空格，直接 starts_with 即可。）
    if (t.starts_with("- [ ]") || t.starts_with("- [x]") || t.starts_with("- [X]"))
        && marker_followed_by_ws(t, 5)
    {
        return Some("- [ ] ");
    }
    if t.starts_with("- ") || t.starts_with("* ") || t.starts_with("+ ") {
        return Some("- ");
    }
    if is_ordered_marker(t) {
        return Some("1. ");
    }
    None
}

/// 回车续行使用的标记：与 `list_marker_of` 相同，但**有序列表编号 +1**
/// （`1. ` → `2. `，`1.1 ` → `1.2 `，`1.2.1 ` → `1.2.2 `），任务统一为未勾选 `- [ ] `。
pub(super) fn continuation_marker(t: &str) -> Option<String> {
    if (t.starts_with("- [ ]") || t.starts_with("- [x]") || t.starts_with("- [X]"))
        && marker_followed_by_ws(t, 5)
    {
        return Some("- [ ] ".to_string());
    }
    if t.starts_with("- ") {
        return Some("- ".to_string());
    }
    if t.starts_with("* ") {
        return Some("* ".to_string());
    }
    if t.starts_with("+ ") {
        return Some("+ ".to_string());
    }
    if let Some(chain) = ordered_chain_of(t) {
        // 末端递增（饱和加法防 u64 溢出）；解析失败（超长数字）回退为 1。
        let mut next = chain.clone();
        let last = next.len() - 1;
        next[last] = next[last].saturating_add(1);
        return Some(format_chain(&next));
    }
    None
}

/// 前 `len` 字节（ASCII 前缀）之后必须紧跟空白或已到行尾。
pub(super) fn marker_followed_by_ws(t: &str, len: usize) -> bool {
    t[len..].chars().next().is_none_or(|c| c.is_whitespace())
}

/// 当前行右边界：不含行尾换行；若为 CRLF 行尾，`\r` 也一并剔除
/// （粘贴 Windows 换行文本时可能混入 `\r`）。
pub(super) fn line_right_edge(text: &str, cursor: usize, line_start: usize) -> usize {
    let end = text[cursor..].find('\n').map(|i| cursor + i).unwrap_or(text.len());
    if end > line_start && text.as_bytes()[end - 1] == b'\r' {
        end - 1
    } else {
        end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 复合编号识别：`1.1 `、`1.2.1 ` 是列表标记，`1.1. `（尾点）不是。
    #[test]
    fn compound_chain_detection() {
        assert_eq!(ordered_chain_of("1.1 文本"), Some(vec![1, 1]));
        assert_eq!(ordered_chain_of("1.2.1 文本"), Some(vec![1, 2, 1]));
        assert_eq!(ordered_chain_of("1. 文本"), Some(vec![1]));
        assert_eq!(ordered_chain_of("12) 文本"), Some(vec![12]));
        assert!(ordered_chain_of("1.1. 文本").is_none(), "复合编号带尾点不识别");
        assert!(ordered_chain_of("1.1文本").is_none(), "无空格分隔不识别");
        assert!(ordered_chain_of("普通文本").is_none());
    }
}
