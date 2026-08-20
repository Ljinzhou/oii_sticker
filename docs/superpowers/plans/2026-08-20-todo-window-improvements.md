# Todo Window Improvements Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 完善 Todo 窗口的默认置顶、无限直接子任务，以及时间和重复预设的可见选中态与标题摘要。

**Architecture:** 保持现有 Vue props/events 和 Pinia store 数据流。Rust 负责原生窗口置顶、system config 迁移和数据库层的一层子任务约束；Vue 负责直接子任务列表、当前交互来源状态和格式化摘要。预设来源只存在当前 Todo 详情组件内，数据库仍只保存时间值和重复规则。

**Tech Stack:** Vue 3 Composition API、TypeScript、Vitest、Pinia、Tauri 2、Rust、rusqlite、dayjs。

---

## 文件地图

- Modify `src/utils/todo-dates.ts`: 调整“今天截止”语义，新增日期和重复规则摘要格式化函数。
- Create `src/utils/todo-dates.test.ts`: 覆盖日期预设和重复规则摘要的纯函数测试。
- Modify `src/components/todo/TodoDetail.vue`: 维护当前任务的预设来源，显示固定按钮文字、蓝色选中态和标题摘要。
- Create `src/components/todo/TodoDetail.test.ts`: 覆盖时间/重复按钮交互和切换任务后的自定义状态。
- Modify `src/components/todo/TodoList.vue`: 将一个父任务的全部直接子任务渲染出来，并始终给根任务提供添加子任务入口。
- Create `src/components/todo/TodoList.test.ts`: 覆盖多子任务展示和禁止从子任务创建孙任务。
- Modify `src-tauri/src/db/todo_block_repo.rs`: 移除直接子任务数量上限，保留同便签和一层嵌套校验。
- Modify `src-tauri/src/db/schema.rs`: 将 schema 版本从 7 升到 8，增加 Todo 窗口置顶默认配置和 v7 → v8 迁移。
- Modify `src-tauri/src/lib.rs`: 创建/显示 Todo 窗口时应用配置，并在配置变化时同步已打开的 Todo 窗口。
- Modify `src/components/console/SettingsPanel.vue`: 在 Todo 设置中增加默认置顶复选框。

## Task 1: 日期与重复摘要纯函数

**Files:**
- Create: `src/utils/todo-dates.test.ts`
- Modify: `src/utils/todo-dates.ts`

- [ ] **Step 1: 写“今天截止为次日零点”的失败测试**

在 `src/utils/todo-dates.test.ts` 中固定时间并断言 `duePreset("today", config)` 是下一自然日 `00:00:00`，同时断言提醒一小时后仍为当前时间加一小时：

```ts
import { afterEach, describe, expect, it, vi } from "vitest";
import dayjs from "dayjs";
import { duePreset, formatTodoDate, formatTodoRepeat } from "./todo-dates";

const config = {
  remindTomorrowHour: 9,
  remindNextWeekDow: 1,
  remindNextWeekHour: 9,
  dueTodayHour: 18,
  dueTomorrowHour: 9,
  dueNextWeekDow: 1,
};

afterEach(() => vi.useRealTimers());

describe("Todo 日期与重复格式化", () => {
  it("今天截止预设使用下一自然日零点", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-08-20T13:30:00+08:00"));

    expect(dayjs(duePreset("today", config)).format("YYYY-MM-DD HH:mm")).toBe("2026-08-21 00:00");
  });

  it("重复规则摘要包含间隔和按周一到周日排序的星期", () => {
    expect(formatTodoRepeat(JSON.stringify({ unit: "week", interval: 2, weekdays: [2, 1, 0] }))).toBe("每 2 周的 周一、周二、周日");
    expect(formatTodoRepeat(JSON.stringify({ unit: "month", interval: 1 }))).toBe("每 1 月");
  });

  it("日期摘要包含完整年份和时间", () => {
    expect(formatTodoDate("2026-08-20T14:00:00+08:00")).toBe("2026年8月20日 14:00");
  });
});
```

- [ ] **Step 2: 运行纯函数测试确认失败**

Run: `pnpm vitest run src/utils/todo-dates.test.ts`

Expected: FAIL because `formatTodoRepeat` does not exist and `duePreset("today")` currently uses the configured hour.

- [ ] **Step 3: 实现最小日期和重复摘要逻辑**

在 `src/utils/todo-dates.ts` 中：

- 将 `duePreset("today")` 改为 `toIso(now.add(1, "day").startOf("day"))`。
- 将 `formatTodoDate` 改为 `YYYY年M月D日 HH:mm` 或 `YYYY年M月D日`。
- 新增 `formatTodoRepeat(value)`：解析 `unit`、`interval`、`weekdays`；周规则使用 `[1, 2, 3, 4, 5, 6, 0]` 排序，存在星期时拼接 `每 N 周的 周一、周二`，其他规则返回 `每 N 天/周/月/年`；非法 JSON 返回 `未设置`。

- [ ] **Step 4: 运行测试确认通过**

Run: `pnpm vitest run src/utils/todo-dates.test.ts`

Expected: PASS with 3 tests passing.

- [ ] **Step 5: 提交纯函数改动**

Run:

```powershell
git add src/utils/todo-dates.ts src/utils/todo-dates.test.ts
git commit -m "feat: format todo date and repeat summaries"
```

## Task 2: Todo 详情选择状态和标题摘要

**Files:**
- Create: `src/components/todo/TodoDetail.test.ts`
- Modify: `src/components/todo/TodoDetail.vue`

- [ ] **Step 1: 写时间和重复交互失败测试**

用真实 `TodoDetail` 挂载一个根任务，stub `TodoDatePicker` 和 `RepeatPicker`，断言：

```ts
it("点击一小时后只选中该按钮并在标题显示日期", async () => {
  const wrapper = mount(TodoDetail, { props: { item: rootTodo, presets }, global: { stubs: { TodoDatePicker: true, RepeatPicker: true } } });
  await wrapper.get("button").trigger("click");

  expect(wrapper.findAll(".chips button")[0].classes()).toContain("active");
  expect(wrapper.findAll(".chips button")[3].text()).toBe("自定义");
  expect(wrapper.find(".field-label").text()).toMatch(/^提醒时间 - \\d{4}年\\d{1,2}月\\d{1,2}日 \\d{2}:\\d{2}$/);
});

it("点击今天后标题显示次日零点，点击自定义重复后显示星期摘要", async () => {
  const wrapper = mount(TodoDetail, { props: { item: rootTodo, presets }, global: { stubs: { TodoDatePicker: true, RepeatPicker: true } } });
  const fields = wrapper.findAll(".field");
  await fields[1].findAll(".chips button")[0].trigger("click");
  expect(fields[1].find(".field-label").text()).toContain("截至时间 - ");
  expect(fields[1].findAll(".chips button")[0].classes()).toContain("active");

  await fields[2].findAll(".chips button")[4].trigger("click");
  expect(wrapper.findComponent({ name: "RepeatPicker" }).exists()).toBe(true);
});
```

测试中通过 `wrapper.getComponent(RepeatPicker).vm.$emit("save", JSON.stringify({ unit: "week", interval: 2, weekdays: [2, 1] }))` 模拟自定义选择器保存，并断言标题为 `设置任务重复 - 每 2 周的 周一、周二`、自定义按钮为蓝色。

- [ ] **Step 2: 运行详情测试确认失败**

Run: `pnpm vitest run src/components/todo/TodoDetail.test.ts`

Expected: FAIL because预设按钮目前没有来源状态，按钮文字会显示数据库日期/规则摘要，字段标题仍是静态文字。

- [ ] **Step 3: 实现详情组件的局部来源状态**

在 `TodoDetail.vue` 中：

- 为提醒、截止、重复分别添加 `ref` 来源状态，值域为 `empty`、具体预设值或 `custom`。
- 只 watch `props.item?.id` 来初始化字段和来源状态；任务内容更新导致的同 id 对象替换不能覆盖刚点击的蓝色状态。
- 将 `setReminder`、`setDue`、`setRepeat` 接受第二个来源参数，预设按钮显式传来源，自定义 picker 的 `save` 使用默认 `custom`。
- `repeatLabel` 改用 `formatTodoRepeat`。
- 用 computed 标题替代静态 `field-label` 文本：值存在时显示 `字段名 - 摘要`，否则只显示字段名。
- 自定义按钮固定显示 `自定义`，不再显示 `formatTodoDate` 或重复规则摘要；各按钮的 `active` 只绑定来源状态。
- 保持子任务不显示高级设置和添加子任务入口。

- [ ] **Step 4: 运行详情测试确认通过**

Run: `pnpm vitest run src/components/todo/TodoDetail.test.ts`

Expected: PASS with all interaction assertions passing.

- [ ] **Step 5: 提交详情改动**

Run:

```powershell
git add src/components/todo/TodoDetail.vue src/components/todo/TodoDetail.test.ts
git commit -m "feat: show todo preset selection state"
```

## Task 3: 多个直接子任务

**Files:**
- Create: `src/components/todo/TodoList.test.ts`
- Modify: `src/components/todo/TodoList.vue`
- Modify: `src-tauri/src/db/todo_block_repo.rs`

- [ ] **Step 1: 写 Vue 列表失败测试**

挂载一个根任务和两个 `parent_id` 相同的子任务，断言两个 `.sub-task` 都存在、文本都显示，并且只有根任务有 `.add-child`：

```ts
it("渲染一个根任务下的全部直接子任务", () => {
  const wrapper = mount(TodoList, { props: { items: [root, child1, child2], selectedId: root.id, height: 220 } });

  expect(wrapper.findAll(".sub-task")).toHaveLength(2);
  expect(wrapper.text()).toContain("子任务一");
  expect(wrapper.text()).toContain("子任务二");
  expect(wrapper.findAll(".add-child")).toHaveLength(1);
});
```

- [ ] **Step 2: 运行 Vue 列表测试确认失败**

Run: `pnpm vitest run src/components/todo/TodoList.test.ts`

Expected: FAIL because the current component uses `find` and hides the add-child row once one child exists.

- [ ] **Step 3: 实现按父任务分组的列表渲染**

将 `childOf` 替换为按 `parent_id` 分组的 computed Map；模板对每个根任务执行 `v-for="child in childrenByParent.get(item.id) ?? []"`，每个 child 使用 `child.id` 作为 key。根任务下始终渲染一个 `.add-child` 行并用 `@click.stop` 发出 `createChild`；子任务自身不渲染该行。

- [ ] **Step 4: 运行 Vue 列表测试确认通过**

Run: `pnpm vitest run src/components/todo/TodoList.test.ts`

Expected: PASS.

- [ ] **Step 5: 写 Rust 仓储失败测试**

修改 `src-tauri/src/db/todo_block_repo.rs` 中现有测试，将单子任务断言改为：创建 `child-1`、`child-2`，确认两者 parent 相同；再尝试以 `child-1` 为 parent 创建任务并断言失败。

- [ ] **Step 6: 运行 Rust 仓储测试确认失败**

Run: `cargo test todo_block_repo::tests::creates_parent_and_multiple_direct_children --manifest-path src-tauri/Cargo.toml`

Expected: FAIL at the second direct child because生产代码仍返回“一个父任务最多只能有一个子任务”。

- [ ] **Step 7: 移除后端直接子任务数量上限**

删除 `todo_block_repo::create` 中查询 `EXISTS(SELECT 1 ... parent_id)` 的代码和对应错误；保留父任务存在、同一便签和 `parent.parent_id.is_some()` 的校验。

- [ ] **Step 8: 运行 Rust 仓储测试确认通过**

Run: `cargo test todo_block_repo::tests --manifest-path src-tauri/Cargo.toml`

Expected: PASS，直接子任务测试和高级字段/级联删除测试均通过。

- [ ] **Step 9: 提交多子任务改动**

Run:

```powershell
git add src/components/todo/TodoList.vue src/components/todo/TodoList.test.ts src-tauri/src/db/todo_block_repo.rs
git commit -m "feat: allow multiple todo child tasks"
```

## Task 4: Todo 默认置顶配置和原生窗口同步

**Files:**
- Modify: `src-tauri/src/db/schema.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/components/console/SettingsPanel.vue`

- [ ] **Step 1: 写 schema 迁移失败测试**

在 `schema.rs` 测试中新增 v7 → v8 用例：创建最小 `system_config` 表，写入 `default_todo_always_on_top = '0'`，设置 `PRAGMA user_version = 7`，运行 `run_migrations` 后断言版本为 8 且值仍为 `0`；再用无该配置的连接断言默认写入 `1`。

- [ ] **Step 2: 运行迁移测试确认失败**

Run: `cargo test schema::tests::migrate_v7_to_v8_adds_todo_window_topmost_default --manifest-path src-tauri/Cargo.toml`

Expected: FAIL because schema version仍为 7 and no v7 → v8 migration exists.

- [ ] **Step 3: 实现 schema v8 配置迁移**

在 `schema.rs`：

- 将 `SCHEMA_VERSION` 改为 `8`。
- 在首次安装 defaults 增加 `("default_todo_always_on_top", "1", "Todo 窗口默认是否置顶")`。
- 新增 `migrate_v7_to_v8`，在事务中执行 `INSERT OR IGNORE INTO system_config ...`。
- 在 `run_migrations` 的 v7 分支后调用 `migrate_v7_to_v8`。
- 更新现有 schema 测试中固定的 `7` 期望为 `8`，确保跨版本迁移仍覆盖该新版本。

- [ ] **Step 4: 运行 schema 测试确认通过**

Run: `cargo test schema::tests --manifest-path src-tauri/Cargo.toml`

Expected: PASS.

- [ ] **Step 5: 写原生窗口配置行为的可验证改动前检查**

在实现前确认 `set_config_cmd` 已有 `AppHandle`，Todo 窗口 label 统一为 `todo-<id>`，`create_todo_win` 是唯一 Todo 建窗函数；不引入第二套窗口状态存储。

- [ ] **Step 6: 实现创建、显示和配置变更时的置顶同步**

在 `lib.rs`：

- 将 `create_todo_win(app, id)` 改为 `create_todo_win(app, id, always_on_top)`，建窗后调用 `win.set_always_on_top(always_on_top)`，失败只记录 `tracing::warn!`。
- `open_todo_window_cmd` 读取 `default_todo_always_on_top`，已有窗口显示前和新窗口创建时都应用该值。
- `set_config_cmd` 在 key 为 `default_todo_always_on_top` 时遍历 `app.webview_windows()` 中以 `todo-` 开头的窗口并调用 `set_always_on_top(value == "1")`，单个窗口失败只记录警告。

- [ ] **Step 7: 增加系统设置复选框**

在 Todo 设置面板的时间预设之前增加：

```vue
<label class="row">
  <span>Todo 窗口默认置顶</span>
  <input
    type="checkbox"
    :checked="settings.get('default_todo_always_on_top', '1') === '1'"
    @change="(e) => settings.set('default_todo_always_on_top', (e.target as HTMLInputElement).checked ? '1' : '0')"
  />
</label>
```

- [ ] **Step 8: 运行前端类型检查和 Rust 编译检查**

Run: `pnpm build`

Expected: `vue-tsc --noEmit` and Vite build exit 0.

Run: `cargo check --manifest-path src-tauri/Cargo.toml`

Expected: Rust compile check exit 0.

- [ ] **Step 9: 提交窗口置顶改动**

Run:

```powershell
git add src-tauri/src/db/schema.rs src-tauri/src/lib.rs src/components/console/SettingsPanel.vue
git commit -m "feat: add todo window topmost setting"
```

## Task 5: 全量回归与完成前验证

**Files:**
- No planned source changes; only fix regressions found by the commands below.

- [ ] **Step 1: 运行 Todo 相关 Vitest**

Run: `pnpm vitest run src/components/todo src/stores/todo.test.ts src/utils/todo-dates.test.ts`

Expected: all matching tests pass with zero failures.

- [ ] **Step 2: 运行完整前端测试**

Run: `pnpm test`

Expected: Vitest exits 0 with zero failed tests.

- [ ] **Step 3: 运行完整前端构建**

Run: `pnpm build`

Expected: TypeScript check and Vite production build both exit 0.

- [ ] **Step 4: 运行完整 Rust 测试**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`

Expected: all Rust tests pass. If the local Rust toolchain or native dependencies are unavailable, record the exact failure and do not claim Rust tests passed.

- [ ] **Step 5: 检查变更范围和工作区状态**

Run: `git diff --check; git status --short; git diff HEAD~4 --stat`

Expected: no whitespace errors; only the Todo implementation, its tests, the system setting, and committed design/plan documents are present.

- [ ] **Step 6: 提交最终验证修复（如有）**

只有在前述命令发现并修复了回归时，运行：

```powershell
git add src/components/todo src/utils/todo-dates.ts src/utils/todo-dates.test.ts src/components/console/SettingsPanel.vue src-tauri/src/db/schema.rs src-tauri/src/db/todo_block_repo.rs src-tauri/src/lib.rs
git commit -m "fix: resolve todo regression"
```

只在这些路径中确实包含本次验证期间产生的修复时执行该提交；没有额外修复时不创建空提交。
