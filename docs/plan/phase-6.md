# 阶段 6 — 集成联调（多窗口 + 命令 + 事件）

> 所属总览：[`../PLAN.md`](../PLAN.md) · 状态：⏳ 未开始 · 前置：阶段 5 完成

## 6️⃣ 需要的 Skills / MCP（本阶段）
| 工具 | 用途 | 状态 |
|---|---|---|
| `systematic-debugging` / `diagnose` | 联调期 bug（跨窗口状态不同步、事件丢失、窗口样式失效） | ✅ 已装 |
| `review` / `security_review` | 全量 diff 审查 | ✅ 已装 |
| `agent-browser` | 可选：WebView 侧 UI 自动化冒烟（Tauri 若可接入则用；不行则手动） | ✅ 已装 |
| `context7-mcp`（MCP） | 联调中遇到的 API 行为疑问 | ✅ 已装 |
| `claude-vision-skill` | 联调期界面截图问题定位（透明/布局/文字可读性） | 🆕 新装 |

## 📋 详细步骤
1. 全命令注册：`generate_handler![...]` 完整列表（§6.1 契约），`invoke` 参数/返回与前端 `types.ts` 对齐
2. 事件打通：`push-update`/`alert-active`/`prefs-updated` 在 multi-window 下的广播（`emit_to` 与 `emit` 的选择）
3. 主链路冒烟：新建便签（托盘 + 主控台）→ 编辑 → 提醒设置 → 到点通知 → 三模式切换 → 偏好应用 → 关闭重开恢复（位置/尺寸/模式）
4. 旧库兼容实测：把源 `oi_sticker` 的 `stickers.db` 拷入 `app_data_dir`，验证打开/读写/提醒字段
5. 并发验证：多窗口同时写库（单连接 Mutex + spawn_blocking 无死锁）

## ✅ 验收
- [ ] 主链路全通；多窗口状态一致；重启恢复
- [ ] 旧 stickers.db 兼容读写；无死锁
- [ ] review/security_review 通过

## 完成动作
- 更新 `../PLAN.md` §1 状态表 → 下一步：阶段 7
- **中文 git 提交**，如：`feat: 完成集成联调（命令/事件/多窗口主链路）`
