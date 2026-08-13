//! 斜杠命令检索（拼音 / 首字母 / 中文 / 别名）。
//!
//! 匹配规则（`query` 为空时全部匹配）：
//! 1. 中文名子串：`标题` → 一级标题 / 二级标题 / …；
//! 2. 拼音全拼前缀：`biao ti` 或 `biaoti` → 标题类；
//! 3. 拼音首字母缩写前缀：`bt` → 标题类（`biao ti` → `bt`）；
//! 4. 别名关键词包含：`task` / `todo` → 任务列表。

use super::SlashCommand;

/// 判断 `query` 是否匹配命令。
pub fn matches(cmd: &SlashCommand, query: &str) -> bool {
    let q = query.trim().to_lowercase();
    let q_nospace: String = q.chars().filter(|c| !c.is_whitespace()).collect();
    if q_nospace.is_empty() {
        return true;
    }

    // 1. 中文名子串（忽略大小写，一般中文无大小写）。
    if cmd.name.to_lowercase().contains(&q) {
        return true;
    }

    // 拼音（空格分隔音节）。
    let pinyin = cmd.pinyin.to_lowercase();
    // 2. 全拼子串（含空格变体：`biao ti` / `biaoti` / `biao` 都能命中
    //    `yi ji biao ti`）。
    let pinyin_nospace: String = pinyin.chars().filter(|c| !c.is_whitespace()).collect();
    if pinyin.contains(&q) || pinyin_nospace.contains(&q_nospace) {
        return true;
    }

    // 3. 首字母缩写：`yi ji biao ti` → `yjbt`，按子序列匹配
    //    （`bt` 命中标题，`yjb` 也能命中，`zt` 命中 `yi ji biao ti` 失败）。
    let initials: String = pinyin
        .split_whitespace()
        .filter_map(|s| s.chars().next())
        .collect();
    if is_subsequence(&q_nospace, &initials) {
        return true;
    }

    // 4. 别名关键词（逗号分隔；包含匹配，如 `task` 命中 `task, todo`）。
    for kw in cmd.alias.split(',') {
        let kw = kw.trim().to_lowercase();
        if !kw.is_empty() && kw.contains(&q) {
            return true;
        }
    }

    false
}

/// `needle` 是否为 `haystack` 的字符子序列（按序出现即可）。
fn is_subsequence(needle: &str, haystack: &str) -> bool {
    let mut it = haystack.chars();
    needle.chars().all(|c| it.any(|h| h == c))
}

/// 按 `query` 过滤命令，保持命令表顺序。
pub fn filter<'a>(commands: &'a [SlashCommand], query: &str) -> Vec<&'a SlashCommand> {
    commands.iter().filter(|c| matches(c, query)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slash::commands::builtin_commands;

    fn ids(query: &str) -> Vec<&'static str> {
        let all = builtin_commands();
        filter(&all, query)
            .into_iter()
            .map(|c| c.id)
            .collect()
    }

    /// 空查询返回全部命令。
    #[test]
    fn empty_query_matches_all() {
        assert_eq!(ids("").len(), builtin_commands().len());
        assert_eq!(ids("   ").len(), builtin_commands().len());
    }

    /// 中文子串：`标题` / `标` 命中标题类。
    #[test]
    fn chinese_substring_matches() {
        let r = ids("标题");
        assert!(r.contains(&"heading1"), "{r:?}");
        assert!(r.contains(&"heading2"), "{r:?}");
        assert!(!r.contains(&"bullet-list"), "{r:?}");

        let r = ids("列表");
        assert!(r.contains(&"bullet-list"), "{r:?}");
        assert!(r.contains(&"ordered-list"), "{r:?}");
        assert!(r.contains(&"task-list"), "{r:?}");
    }

    /// 拼音全拼前缀：`biao ti` 与连写 `biaoti` 等价。
    #[test]
    fn pinyin_full_prefix_matches() {
        for q in ["biao ti", "biaoti", "biao"] {
            let r = ids(q);
            assert!(r.contains(&"heading1"), "query={q} → {r:?}");
        }
    }

    /// 首字母缩写：`bt` → 标题、`lb` → 列表、`fkx` → 复选框。
    #[test]
    fn pinyin_initials_matches() {
        let r = ids("bt");
        assert!(r.contains(&"heading1") && r.contains(&"heading2"), "{r:?}");

        let r = ids("lb");
        assert!(r.contains(&"bullet-list") && r.contains(&"ordered-list"), "{r:?}");

        let r = ids("fkx");
        assert!(r.contains(&"task-list"), "{r:?}");
    }

    /// 别名关键词：`task` / `todo` / `code`。
    #[test]
    fn alias_keyword_matches() {
        assert!(ids("task").contains(&"task-list"));
        assert!(ids("todo").contains(&"task-list"));
        assert!(ids("code").contains(&"code-block"));
        assert!(ids("table").contains(&"table"));
    }

    /// 无匹配返回空；大小写不敏感。
    #[test]
    fn no_match_and_case_insensitive() {
        assert!(ids("zzzz").is_empty());
        assert!(ids("BT").contains(&"heading1"));
    }

    /// 命令表 id 唯一且字段非空（接口完整性）。
    #[test]
    fn command_table_integrity() {
        let all = builtin_commands();
        let mut ids_seen = std::collections::HashSet::new();
        for c in &all {
            assert!(!c.id.is_empty(), "id 不能为空");
            assert!(!c.name.is_empty(), "name 不能为空");
            assert!(!c.pinyin.is_empty(), "pinyin 不能为空");
            assert!(ids_seen.insert(c.id), "重复 id: {}", c.id);
        }
    }
}
