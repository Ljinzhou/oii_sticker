# 阶段 5 — Vue UI（全新设计）

> 所属总览：[`../PLAN.md`](../PLAN.md) · 状态：⏳ 未开始 · 前置：阶段 4 完成 · 本阶段为前端主体，工作量最大，建议拆多个子任务提交

## 5️⃣ 需要的 Skills / MCP（本阶段）
| 工具 | 用途 | 状态 |
|---|---|---|
| `frontend-design` | 主设计基线：全局视觉语言 | ✅ 已装 |
| `taste-skill` + `impeccable` + `huashu-design` + `antfu-design` + `web-design-guidelines` | 设计品味组合拳：配色/字体/间距/控件质感/反 AI 味；huashu 提供 20 设计哲学 + 5 维评审流程 | 🆕 新装（重启会话生效） |
| `vue` + `pinia` + `vite` + `vueuse-functions` + `vue-best-practices` + `vue-router-best-practices`（antfu skills） | Vue 3 写法、store 模式、组合式函数、路由、最佳实践对照 | 🆕 新装 |
| `context7-mcp`（MCP） | `markdown-it` + `markdown-it-task-lists`（token.map 源行映射）、`dayjs` 格式化的精确 API | ✅ 已装 |
| `dispatching-parallel-agents` | 并行实现多个独立组件（console 组 / sticker 组 / markdown 组） | ✅ 已装 |
| `claude-vision-skill` | UI 完成后截图做视觉走查（布局/配色/对齐） | 🆕 新装 |

## 📋 详细步骤（按 REWRITE_PLAN §4.2 结构）
1. 脚手架：`pinia` stores（`notes.ts`/`settings.ts`/`prefs.ts`）、`vue-router`（主控台内子页）、`composables/`（`useTauri.ts` 封装 invoke/listen、`useSticker.ts`、`useDraggable.ts`、`useReminder.ts`）、`utils/`（markdown 源行映射、重复规则文案）
2. `components/console/`：`ConsoleList.vue`/`ConsoleListItem.vue`/`ConsoleToolbar.vue`/`SettingsPanel.vue`（列表/预览/提醒/显示隐藏删除）
3. `components/sticker/`：`StickerWindow.vue`（三模式分发容器）/`StickerHeader.vue`（拖动/居中/⚙/×/双击唤醒）/`StickerViewer.vue`（interact：渲染+todo+折叠）/`StickerEditor.vue`（edit：编辑+slash 浮层）/`StickerSettings.vue`（偏好弹层）/`StickerResizeHandle.vue`/`ReminderPicker.vue`
4. `components/markdown/`：`MarkdownView.vue`（markdown-it 渲染 + todo 勾选经 `token.map` → `toggle_todo(id, line)`）、`TaskItem.vue`
5. `components/slash/`：`SlashMenu.vue`/`SlashMenuItem.vue`（浮层 + 键盘导航）
6. `components/common/`：Button/Modal/ColorInput/Slider/Stepper/`DateTimePicker.vue`
7. 三模式状态机（display/interact/edit，含 5s 无操作自动收起、编辑态不收起）接入 `auto_collapse` 语义（generation 计数）
8. 每组件 ≤300-400 行；`v-model`/`defineModel` 双向绑定贯穿；pinia 为单一真相源
9. 每个组件用 taste-skill/impeccable/huashu 的设计原则自查一轮；主控台与便签窗口视觉统一
10. 完成后截图，用 `claude-vision-skill` 视觉走查

## ✅ 验收
- [ ] 三模式语义正确（display=低透明收起、无动画；interact 可勾 todo；edit 可编辑+slash）
- [ ] Markdown 渲染：标题/列表/任务清单/引用/链接/表格/图片；todo 勾选映射回源行并落库
- [ ] 全部组件走 v-model/pinia；无单文件超限；设计无 AI 味（taste/impeccable 评审通过）
- [ ] 窗口按 `display_mode` 应用透明/置顶/任务栏隐藏
- [ ] 视觉走查通过

## 完成动作
- 更新 `../PLAN.md` §1 状态表 → 下一步：阶段 6
- **中文 git 提交**（建议分多次）：如 `feat: 完成主控台UI`、`feat: 完成便签窗口三模式UI`、`feat: 完成markdown渲染与todo勾选`、`feat: 完成slash浮层与偏好设置`
