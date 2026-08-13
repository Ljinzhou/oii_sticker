//! 编辑器智能行为（纯文本变换，与渲染无关）。
//!
//! 本模块为前端编辑器提供纯函数：列表续行 / 智能缩进 / 删除标记 /
//! todo 行定位。与渲染无关，可在 Rust 侧单测。

pub mod edit_backspace;
pub mod edit_enter;
pub mod edit_indent;
pub mod edit_todo;
pub mod list;

pub use edit_todo::toggle_todo_in_content;
