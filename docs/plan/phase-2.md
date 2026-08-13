# 阶段 2 — 核心逻辑（datetime / slash / editing / commands）

> 所属总览：[`../PLAN.md`](../PLAN.md) · 状态：⏳ 未开始 · 前置：阶段 1 完成

## 2️⃣ 需要的 Skills / MCP（本阶段）
| 工具 | 用途 | 状态 |
|---|---|---|
| `explore`（子代理） | 并行提取旧项目规格：`datetime/`（DateTime/parse/repeat：daily/weekly/interval/monthly/yearly 语法与推进算法）、`slash/`（20 条命令表、拼音/首字母/别名匹配、插入纯函数、菜单状态机）、`markdown/edit_*`（indent/enter/backspace/todo 行定位）、`auto_collapse`（generation 计数） | ✅ 已装 |
| `dispatching-parallel-agents` | 并行派 3 个子代理（datetime / slash / editing+auto_collapse） | ✅ 已装 |
| `tdd` / `test-driven-development` | 重复规则、匹配算法、文本变换全部先写测试 | ✅ 已装 |
| `context7-mcp`（MCP） | 如需外部 crate（如拼音匹配用 `pinyin` crate 时查其 API；也可自实现首字母表） | ✅ 已装 |

## 📋 详细步骤
1. `src-tauri/src/datetime/`：`DateTime`（自研、无 chrono）、`parse.rs`（统一 `YYYY-MM-DDTHH:MM:SS` 解析 + 兼容旧 `YYYY-MM-DD HH:MM`）、`repeat.rs`（`compute_next_remind_at`/`advance_from`/`next_occurrence`，上限 366 次追补）
2. `src-tauri/src/slash/`：命令表（约 20 条：标题/列表/代码/引用/链接/图片/表格/折叠框…）、匹配器（中文/拼音/首字母/别名）、插入纯函数、`SlashState` 菜单状态机
3. `src-tauri/src/editing/`：`indent`/`enter`（列表续行，依赖 `markdown/list.rs` 的嵌套列表辅助）/`backspace`（删除标记）/`todo`（todo 行定位）纯文本变换
4. `src-tauri/src/commands.rs`：把上述逻辑封装为业务函数（create/update/delete/set_reminder/clear_reminder/acknowledge_alert/set_config/update_sticker_prefs/reset_sticker_prefs/toggle_todo/slash_query…）——本阶段先做**纯函数层**，`#[tauri::command]` 包装放阶段 6 前完成（或本阶段直接完成亦可）
5. 单测：重复规则全语法用例、匹配器（拼音/首字母/别名）、文本变换边界（空行/嵌套列表/CRLF）
6. `markdown/list.rs`：有序/无序/嵌套列表续行辅助（供 edit_enter 使用）

## ✅ 验收
- [ ] 重复规则计算正确（含跨月/闰年/关机缺口追补上限 366）
- [ ] slash 20 条命令 + 匹配可用；文本变换用例全绿
- [ ] 纯 Rust，零 UI 依赖，测试独立可跑

## 完成动作
- 更新 `../PLAN.md` §1 状态表 → 下一步：阶段 3
- **中文 git 提交**，如：`feat: 完成核心逻辑（datetime/slash/editing/commands）`
