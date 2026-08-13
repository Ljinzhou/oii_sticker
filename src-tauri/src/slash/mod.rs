//! 斜杠命令菜单（Notion 式"/" 菜单）：编辑模式输入 `/` 弹出命令选择；
//! 支持拼音 / 首字母 / 关键词检索，选中后在光标处插入对应 Markdown 模板。

pub mod commands;
pub mod insert;
pub mod matcher;
pub mod state;

use std::sync::Mutex;

/// 一条斜杠命令。
///
/// `insert` 决定选中后在光标处插入什么。入参为光标所在行以 `/` 开头的
/// 已输入内容（不含 `/` 本身，如用户输入 `/bt` 则传 `"bt"`），返回要
/// 插入的完整模板文本；返回 `None` 表示该命令不接受当前输入（菜单中即
/// 置灰 / 不可选）。
pub struct SlashCommand {
    /// 唯一标识（如 `"heading1"`），用于插入逻辑与持久化。
    pub id: &'static str,
    /// 菜单显示名（中文，如 `"一级标题"`）。
    pub name: &'static str,
    /// 拼音全拼（小写，如 `"yijibiaoti"`），用于拼音检索。
    pub pinyin: &'static str,
    /// 别名关键词，逗号分隔（英文 / 缩写 / 常见叫法）。
    pub alias: &'static str,
    /// 分类（菜单中分组显示，如 `"基础"` / `"列表"`）。
    pub category: &'static str,
    /// 菜单右侧提示（如快捷键提示 `"# "`）。
    pub hint: &'static str,
    /// 选中后的插入逻辑（见结构体文档）。
    pub insert: fn(query: &str) -> Option<String>,
}

impl SlashCommand {
    /// 显示名（菜单第一行）。
    pub fn label(&self) -> &str {
        self.name
    }
}

/// 全局命令注册表（内置命令 + 用户自定义命令）。
static REGISTRY: Mutex<Vec<SlashCommand>> = Mutex::new(Vec::new());

/// 注册一条自定义命令（进程内全局生效；重复 id 会被忽略并返回 `false`）。
pub fn register_command(cmd: SlashCommand) -> bool {
    let mut reg = REGISTRY.lock().unwrap();
    if reg.iter().any(|c| c.id == cmd.id) {
        return false;
    }
    reg.push(cmd);
    true
}

/// 全部命令：内置命令在前，自定义命令在后。
pub fn all_commands() -> Vec<SlashCommand> {
    let mut all = commands::builtin_commands();
    let reg = REGISTRY.lock().unwrap();
    all.extend(reg.iter().cloned());
    all
}

impl std::fmt::Debug for SlashCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SlashCommand")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("pinyin", &self.pinyin)
            .field("category", &self.category)
            .finish()
    }
}

// SlashCommand 需要可克隆（供 Vec 复制），但 insert 是函数指针，天然 Copy。
impl Clone for SlashCommand {
    fn clone(&self) -> Self {
        *self
    }
}

impl Copy for SlashCommand {}
