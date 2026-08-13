# 阶段 7 — 测试与对等性验证

> 所属总览：[`../PLAN.md`](../PLAN.md) · 状态：⏳ 未开始 · 前置：阶段 6 完成 · 完成本阶段即项目交付

## 7️⃣ 需要的 Skills / MCP（本阶段）
| 工具 | 用途 | 状态 |
|---|---|---|
| `tdd` / `test` / `test-driven-development` | 全量测试补齐与执行 | ✅ 已装 |
| `explore`（子代理） | 对照旧项目行为清单（三模式、slash、提醒、偏好合并）逐项核对 | ✅ 已装 |
| `requesting-code-review` / `review` | 交付前代码评审 | ✅ 已装 |
| `verification-before-completion` | 完成声明前强制证据核验 | ✅ 已装 |
| `dispatching-parallel-agents` | 并行跑"行为对照"子代理 | ✅ 已装 |
| `claude-vision-skill` | 最终 UI 视觉验收截图 | 🆕 新装 |

## 📋 详细步骤
1. Rust 单测全绿：db 迁移（幂等/回滚/旧库）、datetime 重复规则、slash 匹配、editing 文本变换、scheduler 续期
2. 前端：vitest 组件测试（MarkdownView 的 todo 源行映射、SlashMenu 匹配、三模式状态机）、`vue-tsc --noEmit`、`vite build`
3. 行为对照矩阵：旧项目功能清单 × 新项目逐项核对（含源"非目标"确认：动画/多级待办/已完成聚合/asset:// 不实现）
4. 验收标准（总览 §6）逐条过：建库迁移、CRUD、markdown-it 渲染与 todo 映射、提醒续期与通知、托盘/自启、窗口样式、slash、三模式、GPL v3
5. 打包冒烟：`pnpm tauri build`（debug/CI 可先跳过安装包）
6. 收尾：AGENTS.md/README 更新；`finishing-a-development-branch` 决定集成方式

## ✅ 验收
- [ ] 全部测试绿；行为对照矩阵无遗留差异（除已确认非目标）
- [ ] 总览 §6 全部 ✅；`cargo clippy` 与 `vue-tsc` 干净
- [ ] 依赖红线复核：无 slint/pulldown-cmark/tray-icon/notify-rust/windows-sys；单一 runtime
- [ ] 视觉验收通过

## 完成动作
- 更新 `../PLAN.md` §1 状态表（全部 ✅ 已完成）
- **中文 git 提交**，如：`chore: 完成阶段7测试与对等性验证（全部验收通过）`
