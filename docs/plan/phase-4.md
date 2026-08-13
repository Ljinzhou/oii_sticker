# 阶段 4 — 提醒调度（reminder/scheduler）

> 所属总览：[`../PLAN.md`](../PLAN.md) · 状态：⏳ 未开始 · 前置：阶段 3 完成

## 4️⃣ 需要的 Skills / MCP（本阶段）
| 工具 | 用途 | 状态 |
|---|---|---|
| `context7-mcp`（MCP） | `tauri::async_runtime::spawn` + `tokio::time::interval`（仅用 Tauri re-export）、事件 `app.emit_to` | ✅ 已装 |
| `tdd` / `test-driven-development` | 调度算法（续期/追补）单测 | ✅ 已装 |
| `explore`（子代理） | 提取旧项目 `reminder/scheduler.rs` 的调度语义（扫描周期、命中判定、续期基准、`remind_at` 推进规则）作规格 | ✅ 已装 |

## 📋 详细步骤
1. `src-tauri/src/reminder/scheduler.rs`：10s `interval` 扫描到期项；命中 → `emit_to(sticker-<id>, "sticky://alert-active", true)` + `platform/notify.rs` 弹通知 + `acknowledge_alert` 后事件回 false
2. 续期：以原 `remind_at` 为基准用阶段 2 的 repeat 算法推进；关机错过周期追补（上限 366）
3. 事件常量集中 `src-tauri/src/events.rs`：`PUSH_UPDATE = "sticky://push-update"`、`ALERT_ACTIVE = "sticky://alert-active"`、`PREFS_UPDATED = "sticky://prefs-updated"` + `emit_to_label` 封装
4. `setup` 内 `tauri::async_runtime::spawn(scheduler_loop(...))`；AppState 提供"待扫描列表"缓存
5. 单测：续期正确性（daily/weekly/interval/monthly/yearly + 跨关机缺口）

## ✅ 验收
- [ ] 到点触发：事件 + 系统通知；重复规则续期正确；追补上限 366
- [ ] 全程无第二个 runtime；调度线程不直接触碰窗口（只 emit 事件）

## 完成动作
- 更新 `../PLAN.md` §1 状态表 → 下一步：阶段 5
- **中文 git 提交**，如：`feat: 完成提醒调度（scheduler/事件/续期追补）`
