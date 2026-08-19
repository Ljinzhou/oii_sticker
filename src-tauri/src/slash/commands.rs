//! 内置斜杠命令表。
//!
//! 所有命令均为「纯模板插入」：选中后把光标前的 `/词` 替换为模板文本。
//! 自定义命令请通过 [`crate::slash::register_command`] 注册。

use super::SlashCommand;

/// 内置命令列表（顺序即菜单展示顺序，按分类聚合）。
pub fn builtin_commands() -> Vec<SlashCommand> {
    vec![
        // ── 基础 ──
        SlashCommand {
            id: "heading1",
            name: "一级标题",
            pinyin: "yi ji biao ti",
            alias: "h1, heading1, 标题",
            category: "基础",
            hint: "# ",
            insert: |_| Some("# ".to_string()),
        },
        // ── 功能 ──
        SlashCommand {
            id: "todo-block", name: "添加 Todo 块", pinyin: "tian jia todo kuai",
            alias: "todo, task, 待办, 任务", category: "功能", hint: "/todo",
            insert: |_| Some(String::new()),
        },
        SlashCommand {
            id: "show-done", name: "显示已完成任务", pinyin: "xian shi yi wan cheng ren wu",
            alias: "done, completed, 已完成", category: "功能", hint: "/show-done",
            insert: |_| Some(String::new()),
        },
        SlashCommand {
            id: "heading2",
            name: "二级标题",
            pinyin: "er ji biao ti",
            alias: "h2, heading2, 标题",
            category: "基础",
            hint: "## ",
            insert: |_| Some("## ".to_string()),
        },
        SlashCommand {
            id: "heading3",
            name: "三级标题",
            pinyin: "san ji biao ti",
            alias: "h3, heading3, 标题",
            category: "基础",
            hint: "### ",
            insert: |_| Some("### ".to_string()),
        },
        SlashCommand {
            id: "heading4",
            name: "四级标题",
            pinyin: "si ji biao ti",
            alias: "h4, heading4, 标题",
            category: "基础",
            hint: "#### ",
            insert: |_| Some("#### ".to_string()),
        },
        SlashCommand {
            id: "heading5",
            name: "五级标题",
            pinyin: "wu ji biao ti",
            alias: "h5, heading5, 标题",
            category: "基础",
            hint: "##### ",
            insert: |_| Some("##### ".to_string()),
        },
        SlashCommand {
            id: "heading6",
            name: "六级标题",
            pinyin: "liu ji biao ti",
            alias: "h6, heading6, 标题",
            category: "基础",
            hint: "###### ",
            insert: |_| Some("###### ".to_string()),
        },
        SlashCommand {
            id: "divider",
            name: "分隔线",
            pinyin: "feng ge xian",
            alias: "hr, rule, ---, 分割线",
            category: "基础",
            hint: "---",
            insert: |_| Some("---\n\n".to_string()),
        },
        // ── 列表 ──
        SlashCommand {
            id: "bullet-list",
            name: "无序列表",
            pinyin: "wu xu lie biao",
            alias: "ul, bullet, list, 列表",
            category: "列表",
            hint: "- ",
            insert: |_| Some("- ".to_string()),
        },
        SlashCommand {
            id: "ordered-list",
            name: "有序列表",
            pinyin: "you xu lie biao",
            alias: "ol, ordered, number, 编号",
            category: "列表",
            hint: "1. ",
            insert: |_| Some("1. ".to_string()),
        },
        SlashCommand {
            id: "task-list",
            name: "任务列表",
            pinyin: "ren wu lie biao",
            alias: "todo, task, checkbox, 复选框, 待办, fkx",
            category: "列表",
            hint: "- [ ] ",
            insert: |_| Some("- [ ] ".to_string()),
        },
        SlashCommand {
            id: "quote",
            name: "引用",
            pinyin: "yin yong",
            alias: "blockquote, quote, 引用块",
            category: "列表",
            hint: "> ",
            insert: |_| Some("> ".to_string()),
        },
        // ── 代码 ──
        SlashCommand {
            id: "code-block",
            name: "代码块",
            pinyin: "dai ma kuai",
            alias: "code, fence, 代码",
            category: "代码",
            hint: "```",
            insert: |_| Some("```\n\n```".to_string()),
        },
        SlashCommand {
            id: "inline-code",
            name: "行内代码",
            pinyin: "xing nei dai ma",
            alias: "code, inline, 代码",
            category: "代码",
            hint: "`code`",
            insert: |_| Some("`代码`".to_string()),
        },
        // ── 高级 ──
        SlashCommand {
            id: "bold",
            name: "加粗",
            pinyin: "jia cu",
            alias: "bold, strong, b, 粗体",
            category: "高级",
            hint: "**text**",
            insert: |_| Some("****".to_string()),
        },
        SlashCommand {
            id: "italic",
            name: "斜体",
            pinyin: "xie ti",
            alias: "italic, em, i, 强调",
            category: "高级",
            hint: "*text*",
            insert: |_| Some("**".to_string()),
        },
        SlashCommand {
            id: "strike",
            name: "删除线",
            pinyin: "shan chu xian",
            alias: "strike, del, s",
            category: "高级",
            hint: "~~text~~",
            insert: |_| Some("~~~~".to_string()),
        },
        SlashCommand {
            id: "link",
            name: "链接",
            pinyin: "lian jie",
            alias: "link, url, href",
            category: "高级",
            hint: "[text](url)",
            insert: |_| Some("[链接文字](https://)".to_string()),
        },
        SlashCommand {
            id: "image",
            name: "图片",
            pinyin: "tu pian",
            alias: "image, img, 图",
            category: "高级",
            hint: "![alt](url)",
            insert: |_| Some("![图片描述](https://)".to_string()),
        },
        SlashCommand {
            id: "details",
            name: "折叠框",
            pinyin: "zhe die kuang",
            alias: "details, collapse, 折叠",
            category: "高级",
            hint: "<details>",
            insert: |_| {
                Some("<details>\n<summary>标题</summary>\n\n内容\n</details>".to_string())
            },
        },
        SlashCommand {
            id: "table",
            name: "表格",
            pinyin: "biao ge",
            alias: "table, grid, 表",
            category: "高级",
            hint: "| 列 |",
            insert: |_| Some("| 列1 | 列2 |\n| --- | --- |\n|  |  |".to_string()),
        },
    ]
}
