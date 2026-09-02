import { describe, it, expect, vi } from "vitest";
import { renderMarkdown, hexToRgba, mathInstancePromise, normalizeCompoundLists } from "./markdown";
import type { TodoBlock } from "../types";

describe("renderMarkdown", () => {
  it("渲染基础 Markdown（标题/粗体/列表）", () => {
    const html = renderMarkdown("# 标题\n\n**加粗** 与 `代码`");
    expect(html).toContain("<h1>标题</h1>");
    expect(html).toContain("<strong>加粗</strong>");
    expect(html).toContain("<code>代码</code>");
  });

  it("任务清单 checkbox 带 data-line 源行号", () => {
    const content = "- [ ] 任务一\n- [x] 任务二";
    const html = renderMarkdown(content);
    // 第 0 行未勾选
    expect(html).toContain(`data-line="0"`);
    expect(html).not.toContain(`data-line="0" checked`);
    // 第 1 行已勾选
    expect(html).toContain(`data-line="1" checked`);
  });

  it("普通列表不生成 checkbox", () => {
    const html = renderMarkdown("- 普通项");
    expect(html).not.toContain("task-checkbox");
  });

  it("引用块与有序列表渲染", () => {
    const html = renderMarkdown("> 引用\n\n1. 第一\n2. 第二");
    expect(html).toContain("<blockquote>");
    expect(html).toContain("<ol>");
    expect(html).toContain("第一");
  });

  it("数学公式 $..$ 渲染为 math-inline 容器（含真实 SVG 输出）", async () => {
    await mathInstancePromise;
    const html = renderMarkdown("公式 $E=mc^2$ 测试");
    expect(html).toContain("math-inline");
    expect(html).toContain("data-tex=");
    expect(html).toContain("E=mc^2");
    // 字体预加载生效：SVG 实际输出（mjx-container 内包含 <svg> 与字形 path）
    expect(html).toContain("mjx-container");
    expect(html).toContain("<svg");
    expect(html).toContain('d="');
  });

  it("块级公式 $$..$$ 渲染为 math-block 容器", async () => {
    await mathInstancePromise;
    const html = renderMarkdown("$$\n\\frac{1}{2}\n$$");
    expect(html).toContain("math-block");
  });

  // ── 三层结构：块(第0层) → 父任务(第1层) → 子任务(第2层) ──
  // 块由 / 菜单创建，只提供卡头标题，自身不作为任务条目渲染。
  const B = "b1"; // 块 id

  /** 块（第 0 层）：parent_id = null。 */
  function mkBlock(id: string, blockTitle: string, over: Partial<TodoBlock> = {}): TodoBlock {
    return { id, sticker_id: 1, title: "", block_title: blockTitle, description: null, is_completed: false, parent_id: null, reminder_at: null, due_at: null, repeat_rule: null, created_at: "", updated_at: "", ...over };
  }
  /** 父任务（第 1 层）：parent_id = 块 id。 */
  function mkTask(id: string, title: string, over: Partial<TodoBlock> = {}): TodoBlock {
    return { id, sticker_id: 1, title, block_title: "", description: null, is_completed: false, parent_id: B, reminder_at: null, due_at: null, repeat_rule: null, created_at: "", updated_at: "", ...over };
  }
  /** 子任务（第 2 层）：parent_id = 父任务 id。 */
  function mkSub(id: string, title: string, parentId: string, over: Partial<TodoBlock> = {}): TodoBlock {
    return { id, sticker_id: 1, title, block_title: "", description: null, is_completed: false, parent_id: parentId, reminder_at: null, due_at: null, repeat_rule: null, created_at: "", updated_at: "", ...over };
  }
  const emptyUi = { folds: {}, subs: {}, doneFolds: {}, doneSrc: {} };

  it("受控 Todo 标记渲染为卡片，任意 HTML 不会透传", () => {
    const html = renderMarkdown(`<todo-block id="${B}"></todo-block>\n\n<script>alert(1)</script>`, [mkBlock(B, ""), mkTask("t-1", "购物")]);
    expect(html).toContain(`data-todo-id="${B}"`);
    expect(html).toContain("购物");
    expect(html).not.toContain("<script>");
  });

  it("块自身不作为任务显示，计数只统计任务", () => {
    const html = renderMarkdown(`<todo-block id="${B}"></todo-block>`, [mkBlock(B, "我的块", { title: "块自己的标题" }), mkTask("t-1", "任务一")]);
    // 卡头是块名
    expect(html).toContain('<span class="tb-title">我的块</span>');
    // 块自身的 title 不应作为任务行出现
    expect(html).not.toContain("块自己的标题");
    // 只有 1 条任务 → 1 个复选框，计数 0 / 1
    expect((html.match(/todo-task-checkbox/g) ?? []).length).toBe(1);
    expect(html).toContain("0 / 1");
  });

  it("空块（无任何任务）显示「暂无任务」且计数为 0 / 0", () => {
    const html = renderMarkdown(`<todo-block id="${B}"></todo-block>`, [mkBlock(B, "空块")]);
    expect(html).toContain("暂无任务");
    expect(html).toContain("0 / 0");
    expect((html.match(/todo-task-checkbox/g) ?? []).length).toBe(0);
  });

  it("未命名块（block_title 为空）卡头显示“未命名任务”，不冒用任务名", () => {
    const html = renderMarkdown(`<todo-block id="${B}"></todo-block>`, [mkBlock(B, ""), mkTask("t-1", "测试任务1")]);
    expect(html).toContain('<span class="tb-title">未命名任务</span>');
    expect(html).not.toContain('<span class="tb-title">测试任务1</span>');
    // 任务行本身仍显示任务名
    expect(html).toContain("测试任务1");
  });

  it("提醒已触发的父任务：行与卡片带高亮类和徽标", () => {
    const task = mkTask("t-1", "购物", { reminder_at: "2026-08-25T02:00:00.000Z", reminded_at: "2026-08-25T02:00:05.000Z" });
    const html = renderMarkdown(`<todo-block id="${B}"></todo-block>`, [mkBlock(B, ""), task]);
    expect(html).toContain("tb-reminded");
    expect(html).toContain("todo-block-reminded");
    expect(html).toContain("提醒中");
    expect(html).not.toContain("tb-overdue");
  });

  it("截止已过未完成：行与卡片带逾期类，完成后消失", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-08-26T00:00:00Z"));
    const task = mkTask("t-1", "交报告", { due_at: "2026-08-25T00:00:00.000Z", due_notified_at: "2026-08-25T00:00:05.000Z" });
    const html = renderMarkdown(`<todo-block id="${B}"></todo-block>`, [mkBlock(B, ""), task]);
    expect(html).toContain("tb-overdue");
    expect(html).toContain("todo-block-overdue");
    expect(html).toContain("已逾期");
    const done = renderMarkdown(`<todo-block id="${B}"></todo-block>`, [mkBlock(B, ""), { ...task, is_completed: true }]);
    expect(done).not.toContain("tb-overdue");
    expect(done).not.toContain("tb-reminded");
    vi.useRealTimers();
  });

  it("正常任务卡片不带任何高亮标记", () => {
    const html = renderMarkdown(`<todo-block id="${B}"></todo-block>`, [mkBlock(B, ""), mkTask("t-1", "普通")]);
    expect(html).not.toContain("tb-reminded");
    expect(html).not.toContain("tb-overdue");
    expect(html).not.toContain("tb-flag");
  });

  it("每个标记渲染自己的独立卡片：一个便签可挂任意多个块", () => {
    const blockA = mkBlock("b1", "块A");
    const taskA = mkTask("t-a", "任务一", { parent_id: "b1" });
    const subA = mkSub("t-a1", "子任务1", "t-a");
    const blockB = mkBlock("b2", "块B");
    const taskB = mkTask("t-b", "任务二", { parent_id: "b2" });
    const html = renderMarkdown('<todo-block id="b1"></todo-block>\n\n<todo-block id="b2"></todo-block>', [blockA, taskA, subA, blockB, taskB]);
    expect((html.match(/todo-block-card/g) ?? []).length).toBe(2);
    expect(html).toContain("任务一");
    expect(html).toContain("子任务1");
    expect(html).toContain("任务二");
    expect(html).toContain("块A");
    expect(html).toContain("块B");
    // 各卡计数只统计自己的任务树：b1 → 2 条（父+子），b2 → 1 条
    expect(html).toContain("0 / 2");
    expect(html).toContain("0 / 1");
  });

  it("只有「有子任务的父任务」带折叠按钮，无子任务的不带", () => {
    const withKid = mkTask("t-1", "有子任务");
    const kid = mkSub("t-2", "子", "t-1");
    const noKid = mkTask("t-9", "无子任务");
    const html = renderMarkdown(`<todo-block id="${B}"></todo-block>`, [mkBlock(B, ""), withKid, kid, noKid]);
    expect(html).toContain('data-caret="t-1"');
    expect(html).not.toContain('data-caret="t-9"');
    // 无子任务的行用占位保持对齐（同宽但不可见）
    expect(html).toContain("tb-caret-placeholder");
  });

  it("卡头折叠：folds 置位后列表隐藏且箭头反向；未置位默认展开", () => {
    const base = `<todo-block id="${B}"></todo-block>`;
    const blocks = [mkBlock(B, ""), mkTask("t-1", "任务")];
    expect(renderMarkdown(base, blocks)).not.toContain(" hidden");
    expect(renderMarkdown(base, blocks).indexOf("▾")).toBeGreaterThan(-1);
    const folded = renderMarkdown(base, blocks, false, { ...emptyUi, folds: { [B]: true } });
    expect(folded).toContain('<ul class="tb-list" hidden>');
    expect(folded.indexOf("▸")).toBeGreaterThan(-1);
    expect(folded).toContain(`data-fold="${B}"`);
  });

  it("父任务折叠：subs 置位后其子任务不渲染，父任务本身仍在", () => {
    const parent = mkTask("t-1", "父任务");
    const kid = mkSub("t-2", "子任务", "t-1");
    const other = mkTask("t-3", "另一父任务");
    const html = renderMarkdown(`<todo-block id="${B}"></todo-block>`, [mkBlock(B, ""), parent, kid, other], false, { ...emptyUi, subs: { "t-1": true } });
    expect(html).toContain("父任务");
    // 注意：折叠按钮 tooltip 是"显示/隐藏子任务"，不能只断言 not.toContain("子任务")，
    // 要精确匹配任务名节点。
    expect(html).not.toContain('<span class="tb-name">子任务</span>');
    expect(html).toContain('<span class="tb-name">另一父任务</span>');
    // 箭头方向反映隐藏状态
    expect(html.indexOf("▸")).toBeGreaterThan(-1);
    // 计数仍按全部任务算（折叠不影响计数）
    expect(html).toContain("0 / 3");
  });

  it("已完成任务卡：显示完成时刻、来源下拉与折叠", () => {
    const rootA: TodoBlock = { id: "t-1", sticker_id: 1, title: "任务一", block_title: "块A", description: null, is_completed: true, parent_id: null, reminder_at: null, due_at: null, repeat_rule: null, completed_at: "2026-08-26 12:01:35", created_at: "", updated_at: "" };
    const rootB: TodoBlock = { ...rootA, id: "t-3", title: "任务二", block_title: "块B", is_completed: false, completed_at: null };
    const base = "<show-done></show-done>";
    // 默认：全部来源，仅完成任务 + 完成时刻
    const html = renderMarkdown(base, [rootA, rootB]);
    expect(html).toContain("已完成 1");
    expect(html).toContain("2026-08-26 12:01:35");
    expect(html).toContain('class="sd-source"');
    expect(html).toContain(">全部任务</option>");
    expect(html).toContain(">块A</option>");
    expect(html).not.toContain("任务二</span>");
    // 来源切换到块B（无完成项）→ 列表为空但下拉选中块B
    const picked = renderMarkdown(base, [rootA, rootB], false, { folds: {}, subs: {}, doneFolds: {}, doneSrc: { s1: "t-3" } }, "s1");
    expect(picked).toContain("已完成 0");
    expect(picked).toContain('value="t-3" selected');
    // 折叠：doneFolds 置位 → 列表 hidden
    const folded = renderMarkdown(base, [rootA], false, { folds: {}, subs: {}, doneFolds: { s1: true }, doneSrc: {} }, "s1");
    expect(folded).toContain('<ul class="db-list" hidden>');
    expect(folded).toContain('data-donefold="s1"');
  });
});

describe("normalizeCompoundLists", () => {
  it("复合编号行转为嵌套列表语法", () => {
    expect(normalizeCompoundLists("1. 文本\n1.1 子项")).toBe("1. 文本\n  1. 子项");
    expect(normalizeCompoundLists("1. 文本\n1.1 子项\n1.1.1 深层")).toBe(
      "1. 文本\n  1. 子项\n    1. 深层",
    );
    expect(normalizeCompoundLists("1.1. 尾点文本")).toBe("  1. 尾点文本");
  });

  it("单级编号与普通行不变", () => {
    expect(normalizeCompoundLists("1. 文本\n2. 文本\n普通行")).toBe("1. 文本\n2. 文本\n普通行");
  });

  it("围栏代码块内不转换", () => {
    const md = "```\n1.1 code line\n```";
    expect(normalizeCompoundLists(md)).toBe(md);
  });
});

describe("hexToRgba", () => {
  it("转换 hex 为 rgba", () => {
    expect(hexToRgba("#FF5733", 0.5)).toBe("rgba(255, 87, 51, 0.5)");
    expect(hexToRgba("FF5733", 1)).toBe("rgba(255, 87, 51, 1)");
  });

  it("非法输入回退默认色", () => {
    expect(hexToRgba("bad", 0.5)).toBe("rgba(255, 255, 255, 0.5)");
  });
});
