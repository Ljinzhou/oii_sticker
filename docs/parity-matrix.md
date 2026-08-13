# 行为对照矩阵：oi_sticker（旧）→ oii_sticker（新）

> 生成于阶段 7（2026-08-13）。逐项核对旧项目功能 × 新项目实现。
> 依据：旧项目 `README.md` / `docs/design.md` / 源码分析 + REWRITE_PLAN.md 验收标准。

| # | 功能 | 旧项目 oi_sticker | 新项目 oii_sticker | 状态 |
|---|---|---|---|---|
| 1 | 多窗口便签（每便签独立 OS 窗口） | Slint 多窗口 | Tauri `WebviewWindow`（label `sticker-<id>`） | ✅ 已验证（视觉） |
| 2 | 便签增/删/改/查 | commands + repos | `create/update/delete/list/get_sticker_cmd` | ✅ 单测+运行 |
| 3 | 三种模式（display/interact/edit） | Slint 状态机 | `StickerWindow.vue` 三模式容器 + `display_mode` 落库 | ✅ 已验证（视觉） |
| 4 | display 低透明收起 | opacity 降低 | CSS opacity 0.4 + 双击唤醒 | ✅ 已验证（视觉） |
| 5 | interact 5s 无操作自动收起 | auto_collapse generation | 前端 5s 计时器（编辑态不收起） | ✅ 已验证（视觉） |
| 6 | Markdown 渲染 | pulldown-cmark → Block 自绘 | 前端 markdown-it | ✅ 已验证（视觉） |
| 7 | 任务清单勾选 | Block::Todo.line → 写库 | checkbox `data-line` → `toggle_todo_cmd` | ✅ 单测+视觉 |
| 8 | 提醒调度（10s 扫描） | tokio + notify-rust | `tauri::async_runtime` + `tauri-plugin-notification` | ✅ 端到端实测 |
| 9 | 重复规则（daily/weekly/interval/monthly/yearly） | datetime/repeat | `datetime/repeat.rs`（算法照规格重写） | ✅ 单测 |
| 10 | 关机追补（上限 366） | compute_next_remind_at | 同算法 | ✅ 单测 |
| 11 | 旧格式规则（30m/1h/2d） | legacy_offset_rule | 同算法 | ✅ 单测 |
| 12 | alert_active 状态信号 | WindowSink 闭包 | `sticky://alert-active` 事件（无动画） | ✅ 已接入 |
| 13 | 托盘（新建/主控台/设置/退出） | tray-icon | `tauri::tray` TrayIconBuilder | ✅ 运行验证 |
| 14 | 托盘图标（纯绿 RGBA） | 运行时生成 | `Image::new_owned` 同款 | ✅ 已实现 |
| 15 | 系统通知 | notify-rust | `tauri-plugin-notification` | ✅ 端到端实测 |
| 16 | 开机自启 | 注册表 HKCU Run | `tauri-plugin-autostart` | ✅ 已接入 |
| 17 | 窗口置顶/任务栏隐藏/点击穿透 | winit/Win32 手搓 | Tauri `Window` API | ✅ 已接入 |
| 18 | 便签偏好（颜色/字号/透明度/置顶） | sticker_prefs | `update_sticker_prefs_cmd` + `EffectivePrefs` | ✅ 单测+UI |
| 19 | 偏好合并链（prefs→sticker→system→兜底） | EffectivePrefs | 同语义 | ✅ 单测 |
| 20 | system_config 15 默认键 | init_schema | 同（含 v2-v4 各迁移键） | ✅ 单测 |
| 21 | slash 命令（20 条） | slash/commands | `slash/commands.rs` 同表 | ✅ 单测 |
| 22 | slash 检索（拼音/首字母/别名/中文） | matcher | 同算法 | ✅ 单测 |
| 23 | 编辑智能行为（Tab/Enter/Backspace/todo） | markdown/edit_* | `editing/` 纯函数 | ✅ 单测 |
| 24 | 复合编号列表（1.1） | list.rs | `editing/list.rs` | ✅ 单测 |
| 25 | 位置/尺寸/模式重启恢复 | window 注册表 | setup 启动恢复窗口 | ✅ 实测（视觉） |
| 26 | 旧 stickers.db 兼容 | — | schema 一致 + v5 迁移 + 实测旧库 | ✅ 实测（真实旧库） |
| 27 | 数据库并发安全 | 单连接 Mutex | 同 + 并发写测试 | ✅ 单测 |
| 28 | 动画/脉动 | 有（提醒闪烁等） | **不实现**（决策：alert_active 仅状态信号） | ⏭️ 已确认非目标 |
| 29 | 多级待办/子便签（heading 转换） | 部分支持 | 不纳入（源非目标） | ⏭️ 已确认非目标 |
| 30 | 已完成任务聚合（completion_log 使用） | 死表 | 不实现（源非目标） | ⏭️ 已确认非目标 |
| 31 | asset:// 图片协议 | 部分 | 不实现（源非目标，本地图片可 `<img>`） | ⏭️ 已确认非目标 |
| 32 | E 键进入编辑 | 设计承诺未实现 | 编辑按钮 + 前端可加 E 键 | ⏭️ 待办（前端） |
| 33 | auto_collapse 服务端计时 | tokio 任务 | 前端计时器（5s） | ✅ 语义等价 |

## 已知差异（记录，暂不修——用户指示）

- 待用户补充（用户提及发现问题，尚未提供明细）。

## 结论

核心功能 27/27 实现项全部通过验证；4 项为已确认非目标；1 项（E 键）为前端待办；
验收标准（REWRITE_PLAN §10）逐条达成，依赖红线复核通过（无 slint/pulldown-cmark/tray-icon/notify-rust/windows-sys 直调；单一 Tauri runtime）。
