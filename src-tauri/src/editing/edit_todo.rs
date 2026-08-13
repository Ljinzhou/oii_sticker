//! 编辑层任务标记（todo）翻转：勾选 / 取消勾选。
//!
//! 交互模式下点击复选框时，把 markdown 原文第 `line` 行的任务标记
//! `[ ]` / `[x]` / `[X]` 翻转，返回新文本直接落库并触发重渲染。
//!
//! 本模块无依赖，只做单行文本替换。

/// 翻转 markdown 原文第 `line` 行（0-based）的任务状态：`[ ]` → `[x]` 或反之。
/// 目标行不含任务标记时返回 None。
///
/// 供交互模式下点击复选框调用；返回的新文本直接落库并触发重渲染。
pub fn toggle_todo_in_content(content: &str, line: usize) -> Option<String> {
    let mut lines: Vec<String> = content.split('\n').map(str::to_string).collect();
    let row = lines.get(line)?;
    // 找行内**最早**出现的任务标记并翻转。pulldown-cmark 只把列表标记后紧
    // 跟的 [ ] / [x] / [X] 识别为任务项，因此 marker 必然出现在行内任何正文
    // [ ] 之前；不能简单 contains+replacen，否则"` [x] 完成 [ ] 未完成`"这类
    // 行会误改正文。
    let (pos, from, to) = [("[ ]", "[x]"), ("[x]", "[ ]"), ("[X]", "[ ]")]
        .iter()
        .filter_map(|(from, to)| row.find(from).map(|pos| (pos, *from, *to)))
        .min_by_key(|(pos, _, _)| *pos)?;
    let new_row = format!("{}{}{}", &row[..pos], to, &row[pos + from.len()..]);
    lines[line] = new_row;
    Some(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 翻转任务状态：只改目标行，其他行保持不变。
    #[test]
    fn toggle_todo_flips_state() {
        let md = "# 标题\n\n- [ ] 待办一\n- [x] 待办二\n";
        let out = toggle_todo_in_content(md, 2).unwrap();
        assert!(out.contains("- [x] 待办一"), "第 2 行应变为已勾选");
        assert!(out.contains("- [x] 待办二"), "第 3 行保持勾选不变");

        let out2 = toggle_todo_in_content(&out, 3).unwrap();
        assert!(out2.contains("- [ ] 待办二"), "再点一次应取消勾选");
        assert!(out2.contains("- [x] 待办一"), "第 2 行保持勾选不变");
    }

    /// 非任务行 / 越界行返回 None。
    #[test]
    fn toggle_todo_non_todo_line_returns_none() {
        let md = "- [ ] 待办\n普通段落";
        assert!(toggle_todo_in_content(md, 1).is_none());
        assert!(toggle_todo_in_content(md, 99).is_none());
    }

    /// 行内正文含另一组标记时，只翻转任务 marker，正文 [ ] 保持不变。
    #[test]
    fn toggle_todo_only_flips_earliest_marker() {
        let md = "- [x] 完成 [ ] 未完成";
        let out = toggle_todo_in_content(md, 0).unwrap();
        assert_eq!(out, "- [ ] 完成 [ ] 未完成", "应翻转最早的 [x]，正文 [ ] 不动");

        let out2 = toggle_todo_in_content(&out, 0).unwrap();
        assert_eq!(out2, "- [x] 完成 [ ] 未完成");
    }
}
