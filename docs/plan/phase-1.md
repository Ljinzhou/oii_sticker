# 阶段 1 — 数据层（models + db）

> 所属总览：[`../PLAN.md`](../PLAN.md) · 状态：⏳ 未开始 · 前置：验证 Demo 通过

## 1️⃣ 需要的 Skills / MCP（本阶段）
| 工具 | 用途 | 状态 |
|---|---|---|
| `explore`（子代理） | 并行分析旧项目 `db/schema.sql`、`db/schema.rs`、`models.rs`、4 个 repo，**提取精确 schema 与迁移逻辑作为需求规格**（不迁移代码，只提取规格） | ✅ 已装 |
| `dispatching-parallel-agents` | 并行派 4 个子代理分别提取 sticker_repo/config_repo/prefs_repo/todo_repo 的行为规格 | ✅ 已装 |
| `context7-mcp`（MCP） | `rusqlite 0.40` API（`Connection`/`execute_batch`/`user_version`/WAL）、Tauri `app.path().app_data_dir()` | ✅ 已装 |
| `sql-optimization` | schema 设计审查（索引、`ON DELETE`、外键） | ✅ 已装 |
| `tdd` / `test-driven-development` | 迁移逻辑与 repo 单测（红-绿-重构） | ✅ 已装 |
| `pnpm`（antfu skill） | 如涉及构建脚本 | 🆕 新装 |

## 📋 详细步骤
1. 派 `explore` 子代理提取旧项目规格：`schema.sql` 全部表结构（stickers/sticker_attrs/todo_items/completion_log/assets/system_config/sticker_prefs）+ `user_version=5` 迁移步骤 + `models.rs` 全部结构体（`Sticker`/`StickerMode`/`StickerAttrs`/`StickerPrefs`/`EffectivePrefs`/`TodoItem`/`SystemConfig` 及其默认值/合并语义）
2. `src-tauri/src/models.rs`：全新实现上述结构体（serde 序列化，供前端 `invoke` 传参）；保留 `EffectivePrefs` 合并链（prefs → sticker → system → 兜底）
3. `src-tauri/src/db/`：`connection.rs`（`app_data_dir()/stickers.db`、WAL、foreign_keys、单连接 `Arc<Mutex<Connection>>`）、`schema.rs`（事务化幂等迁移，`PRAGMA user_version` 对齐源 v5）、`sticker_repo.rs`、`config_repo.rs`、`prefs_repo.rs`、`todo_repo.rs`、`asset_repo.rs`（补源缺失）
4. `state.rs` 雏形：`AppState { conn: Arc<Mutex<Connection>>, cache: RwLock<...> }` + `with_conn(|c| ...)`（内部 `spawn_blocking`）
5. `tauri::State` 托管：`setup` 内 `app.manage(AppState::new(&app)?)`；建库/迁移在启动时执行
6. 单测（TDD）：迁移幂等性（重复执行 user_version 不前进、数据不丢）、CRUD 往返、`EffectivePrefs` 合并优先级、旧库（源 schema v5 生成的文件）可打开
7. 临时命令暴露：`db_health()`（返回版本号/表清单）供前端验证

## ✅ 验收
- [ ] 启动自动建库 + 迁移到 `user_version=5`；重复启动幂等
- [ ] 用源项目旧 `stickers.db` 文件复制到 `app_data_dir` 可正常打开（阶段 7 再正式验证，此处先用单测覆盖）
- [ ] repo 单测全绿；`EffectivePrefs` 合并链与源语义一致
- [ ] 无任何 Slint 依赖

## 完成动作
- 更新 `../PLAN.md` §1 状态表 → 下一步：阶段 2
- **中文 git 提交**，如：`feat: 完成数据层（models/db/迁移/单测全绿）`
