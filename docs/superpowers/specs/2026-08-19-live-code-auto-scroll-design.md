# 及时预览代码块与自动滚动完善设计

## 目标

本轮完善三个直接相关的用户体验问题：及时预览模式能够渲染 fenced 代码块；每个便签可以覆盖系统默认的自动滚动速度；自动滚动在内容底部和顶部之间持续平滑往返，不因低速、小数位移或边界取整而停止。

## 范围与约束

- Markdown 文本继续作为编辑器唯一数据源，代码块只通过 CodeMirror decoration 渲染。
- 保持 Obsidian Live Preview 语义：光标或选区位于 fenced 代码块内时，整块显示 Markdown 源码；离开后整块显示渲染结果。
- 自动滚动速度使用后端已经存在的 `StickerPrefs.auto_scroll_speed` 和 `EffectivePrefs.auto_scroll_speed`，不修改数据库结构或 Rust 数据模型。
- 速度是便签级覆盖项；未覆盖时继续继承系统设置中的默认速度。
- 不改变 Markdown 原文、保存协议、编辑模式切换或现有行内元素行为。

## 代码块渲染

### 范围收集

`liveDecorations.ts` 在遍历 Lezer 语法树时识别完整 `FencedCode` 节点，将其记录为独立块级范围。范围包含起止 fence 和内部代码，语言名从起始 fence 的 info string 中提取并经过安全归一化。

代码块内部不再生成行内 Markdown decoration，避免在代码示例中的星号、链接、公式等内容被二次解析。代码块范围参与现有选区相交判断，只要任一 selection 与范围相交，就不创建 replace decoration。

### 块级 Widget

新增专用 `CodeBlockWidget`，使用现有 Markdown 渲染能力生成完整块级 DOM，而不是复用只抽取段落内容的行内片段方法。Widget 输出语义化的 `pre > code`，保留换行和空白；语言类名仅来自归一化语言标识。

代码块 decoration 使用块级 replace，避免源码与预览同时占位。DOM 样式沿用当前 Markdown 展示区域的代码块视觉规范，并确保长代码横向滚动，不撑破便签宽度。

### 编辑行为

- 光标在代码块外：显示渲染后的完整代码块。
- 光标进入起始 fence、代码正文或结束 fence：整块恢复 Markdown 源码。
- 选区与代码块相交：整块恢复源码。
- 光标离开：立即恢复渲染结果。
- 未闭合 fence：保持源码，避免隐藏用户仍在输入的内容。

## 自动滚动速度设置

`StickerSettings.vue` 在自动滚动开关附近增加速度控件，包含滑块和数值显示。范围为 `5-120 px/s`，步进 `5 px/s`。本地初值来自 `prefs.effective.auto_scroll_speed`，修改后通过现有防抖偏好保存链路写入 `auto_scroll_speed`。

`prefs.applyLocal()` 同步更新 effective preference，使当前窗口无需等待后端返回即可采用新速度。`StickerWindow.vue` 始终优先读取当前便签的 effective speed，缺失时才回退到系统默认速度。

## 往返滚动状态机

将单帧推进抽成不依赖 DOM 的纯函数，输入当前位置、最大位置、方向、速度和帧间隔，输出新位置和新方向。动画循环使用 `requestAnimationFrame` 提供的时间戳计算 `deltaMs`，按 `speed * deltaMs / 1000` 推进，不再假定固定 60 FPS。

状态机采用反射边界：一次位移越过底部时，将超出距离反射到向上方向；越过顶部时同理反射到向下方向。这样即使帧间隔较长，也不会把位置持续钳制在边界。速度较低产生的亚像素位移由状态中的浮点位置累计，再写入 DOM，避免 WebView 对 `scrollTop` 取整后每帧丢失进度。

边界规则：

- `maxScrollTop <= 0` 时位置保持 `0`，动画可等待内容尺寸变化。
- 到达或越过底部后方向变为 `-1`，后续位置明确小于底部。
- 到达或越过顶部后方向变为 `1`，后续位置明确大于顶部。
- 启动或重新启动自动滚动时重置 RAF 时间戳，并从向下方向开始。
- 停止、组件卸载或条件失效时取消 RAF，清理时间戳和累计位置。

## 数据流

1. 设置面板读取 `prefs.effective.auto_scroll_speed`。
2. 用户调整滑块后立即调用 store 的本地 patch，并防抖持久化便签偏好。
3. 便签窗口通过响应式 effective preference 获得新速度。
4. RAF 循环每帧以真实时间差推进纯滚动状态。
5. 边界状态机在顶部和底部切换方向，持续往返。

## 测试策略

遵循 Red-Green-Refactor：

- 先为 fenced 代码块范围和 decoration 写失败测试，覆盖块外渲染、块内恢复、选区相交和未闭合 fence。
- 为 `CodeBlockWidget` 验证 `pre > code`、语言类名、换行和 HTML 转义。
- 为偏好 store 写失败测试，验证 `auto_scroll_speed` 的本地 effective 更新。
- 为滚动纯函数写失败测试，覆盖正常推进、底部反向、顶部反向、低速小数累计、长帧跨边界和无可滚动内容。
- 对设置组件或其提取逻辑验证范围、步进、初值和保存字段。

最终运行全量 Vitest、Vue TypeScript 检查、Vite 构建和 `git diff --check`。UI 验收运行应用并截图检查代码块、速度控件及往返滚动；若视觉识别工具受网络限制，则保留截图并明确报告该限制。
