// 编辑层智能行为（从 src-tauri/src/editing 移植，逻辑与 Rust 单测对齐）：
// - Enter：列表行智能续行（续接标记/有序编号递增/空项退出/行首插空项）
// - Tab：列表嵌套缩进（有序生成复合编号 1.1 / 1.1.1，继承上一行列表类型）
// - Shift+Tab：有序提升一级（目标编号=该层已有项数+1）、无序/任务去掉 2 空格缩进
// - 围栏代码块（```/~~~）内一律退化为普通行为
// 所有函数为纯文本变换，返回 (新文本, 新光标位置)；不适用时返回 null（交给默认行为）。

// ═══════════════ 列表标记识别（list.rs） ═══════════════

/** 解析有序编号链：`1. ` → [1]，`1.1 ` → [1,1]，`1.2.1 ` → [1,2,1]。 */
function orderedChainOf(t: string): number[] | null {
  let i = 0;
  const chain: number[] = [];
  const ds = i;
  while (i < t.length && t[i] >= "0" && t[i] <= "9") i++;
  if (i === ds) return null;
  chain.push(parseInt(t.slice(ds, i), 10));

  while (i < t.length && t[i] === ".") {
    const sep = i;
    i++;
    const ds2 = i;
    while (i < t.length && t[i] >= "0" && t[i] <= "9") i++;
    if (i === ds2) {
      // 尾点：`1.1. ` 之类，回退（多级不允许尾点）
      i = sep;
      break;
    }
    chain.push(parseInt(t.slice(ds2, i), 10));
  }

  if (chain.length === 1) {
    if (t[i] === "." || t[i] === ")") i++;
    else return null;
    return t[i] === " " ? chain : null;
  }
  return t[i] === " " ? chain : null;
}

/** 编号链 → 标记文本：链长 1 → `N. `；链长 ≥ 2 → `N.M.K `。 */
function formatChain(chain: number[]): string {
  const joined = chain.join(".");
  return chain.length === 1 ? `${joined}. ` : `${joined} `;
}

function isOrderedMarker(t: string): boolean {
  return orderedChainOf(t) !== null;
}

/** 前 len 个字符之后必须紧跟空白或已到行尾。 */
function markerFollowedByWs(t: string, len: number): boolean {
  const c = t[len];
  return c === undefined || /\s/.test(c);
}

/** 行首列表标记（含尾随空格）：无序保留原符号，有序保留原数字，任务统一 `- [ ] `。 */
function listMarkerOf(t: string): string | null {
  if (
    (t.startsWith("- [ ]") || t.startsWith("- [x]") || t.startsWith("- [X]")) &&
    markerFollowedByWs(t, 5)
  ) {
    return "- [ ] ";
  }
  if (t.startsWith("- ")) return "- ";
  if (t.startsWith("* ")) return "* ";
  if (t.startsWith("+ ")) return "+ ";
  const chain = orderedChainOf(t);
  return chain ? formatChain(chain) : null;
}

/** 嵌套一层的列表标记（不含缩进）：有序从 1 重排，任务未勾选。 */
function nestedMarkerOf(line: string): string | null {
  const t = line.trimStart();
  if (
    (t.startsWith("- [ ]") || t.startsWith("- [x]") || t.startsWith("- [X]")) &&
    markerFollowedByWs(t, 5)
  ) {
    return "- [ ] ";
  }
  if (t.startsWith("- ") || t.startsWith("* ") || t.startsWith("+ ")) return "- ";
  if (isOrderedMarker(t)) return "1. ";
  return null;
}

/** 回车续行标记：与 listMarkerOf 相同，但有序编号 +1（`1. ` → `2. `，`1.1 ` → `1.2 `）。 */
function continuationMarker(t: string): string | null {
  if (
    (t.startsWith("- [ ]") || t.startsWith("- [x]") || t.startsWith("- [X]")) &&
    markerFollowedByWs(t, 5)
  ) {
    return "- [ ] ";
  }
  if (t.startsWith("- ")) return "- ";
  if (t.startsWith("* ")) return "* ";
  if (t.startsWith("+ ")) return "+ ";
  const chain = orderedChainOf(t);
  if (chain) {
    const next = chain.slice();
    next[next.length - 1] = next[next.length - 1] + 1;
    return formatChain(next);
  }
  return null;
}

/** 当前行右边界（不含换行；CRLF 兼容）。 */
function lineRightEdge(text: string, cursor: number, lineStart: number): number {
  const nl = text.indexOf("\n", cursor);
  const end = nl === -1 ? text.length : nl;
  return end > lineStart && text[end - 1] === "\r" ? end - 1 : end;
}

// ═══════════════ 围栏感知 ═══════════════

/** lineStart 所在行是否在围栏代码块内（``` 只能用 ``` 关闭，~~~ 同理）。 */
function inFenceAt(text: string, lineStart: number): boolean {
  let fence: string | null = null;
  for (const line of text.slice(0, lineStart).split("\n")) {
    const t = line.trimStart();
    if (fence === null) {
      if (t.startsWith("```")) fence = "`";
      else if (t.startsWith("~~~")) fence = "~";
    } else if (
      (fence === "`" && t.startsWith("```")) ||
      (fence === "~" && t.startsWith("~~~"))
    ) {
      fence = null;
    }
  }
  return fence !== null;
}

// ═══════════════ Enter（edit_enter.rs） ═══════════════

export interface EditResult {
  text: string;
  cursor: number;
}

/**
 * 回车智能行为：
 * - 列表项有内容：光标处断行，新行带同缩进与标记（有序编号递增、任务未勾选）；
 *   光标在标记之前（行首）→ 行上方插入空列表项
 * - 列表项为空：删除本行缩进与标记（退出列表）
 * - 围栏内/普通行：返回 null（默认换行）
 */
export function handleEnterAtCursor(text: string, cursor: number): EditResult | null {
  const c = Math.min(cursor, text.length);
  const lineStart = text.lastIndexOf("\n", c - 1) + 1;
  if (inFenceAt(text, lineStart)) return null;
  const lineEnd = lineRightEdge(text, c, lineStart);
  const line = text.slice(lineStart, lineEnd);

  const leading = line.length - line.trimStart().length;
  const trimmed = line.slice(leading);
  const marker = continuationMarker(trimmed);
  if (!marker) return null;

  const markerLen = marker.length;
  const inLine = c - lineStart;

  const rest = trimmed.slice(markerLen);
  if (rest.trim() === "") {
    // 空列表项：删除缩进与标记，退出列表
    return { text: text.slice(0, lineStart) + text.slice(lineEnd), cursor: lineStart };
  }

  const indent = line.slice(0, leading);
  if (inLine <= leading) {
    // 光标在标记之前：行上方插入空列表项
    const newText = `${text.slice(0, lineStart)}${indent}${marker}\n${text.slice(lineStart)}`;
    return { text: newText, cursor: lineStart + leading + markerLen };
  }
  // 光标在标记后：断行续接
  const newText = `${text.slice(0, c)}\n${indent}${marker}${text.slice(c)}`;
  return { text: newText, cursor: c + 1 + leading + markerLen };
}

// ═══════════════ Tab / Shift+Tab（edit_indent.rs） ═══════════════

/** 从当前行往前找最近的有序列表项编号链；遇非空非有序行停止。 */
function findPrevOrderedChain(text: string, lineStart: number): number[] | null {
  let searchStart = lineStart;
  while (searchStart > 0) {
    const prevStart = text.lastIndexOf("\n", searchStart - 2) + 1;
    const prev = text.slice(prevStart, searchStart - 1);
    const t = prev.trimStart();
    const chain = orderedChainOf(t);
    if (chain) return chain;
    if (t !== "") return null;
    searchStart = prevStart;
  }
  return null;
}

function isPrefixOf(prefix: number[], full: number[]): boolean {
  return prefix.length < full.length && full.slice(0, prefix.length).join(".") === prefix.join(".");
}

/** 统计链长 == len 且以 prefix 为前缀的有序列表行数。 */
function countChainRows(text: string, prefix: number[], len: number): number {
  let count = 0;
  for (const line of text.split("\n")) {
    const t = line.trimStart();
    const chain = orderedChainOf(t);
    if (chain && chain.length === len && chain.slice(0, prefix.length).join(".") === prefix.join(".")) {
      count++;
    }
  }
  return count;
}

/**
 * Tab 智能缩进：
 * - 行首/空行：继承上一行列表类型生成嵌套子项（`  - `、`  - [ ] `、`  1. `）；
 *   上一行非列表则插 2 空格
 * - 有序列表行内：缩进一层生成复合编号（`2. ` 按 Tab → `  1.1 `）
 * - 其他列表/普通行：行首插 2 空格
 * - 围栏内：普通 2 空格缩进
 */
export function handleTabAtCursor(text: string, cursor: number): EditResult {
  const c = Math.min(cursor, text.length);
  const lineStart = text.lastIndexOf("\n", c - 1) + 1;

  if (inFenceAt(text, lineStart)) {
    return { text: `${text.slice(0, lineStart)}  ${text.slice(lineStart)}`, cursor: c + 2 };
  }

  const prefix = text.slice(lineStart, c);
  if (prefix.trim() === "") {
    // 行首（或其后全是空白）：生成嵌套列表项
    let marker: string | null = null;
    if (lineStart > 0) {
      const prevStart = text.lastIndexOf("\n", lineStart - 2) + 1;
      marker = nestedMarkerOf(text.slice(prevStart, lineStart - 1));
    }
    const indent = marker ? `  ${marker}` : "  ";
    const newText = `${text.slice(0, lineStart)}${indent}${text.slice(lineStart)}`;
    return { text: newText, cursor: lineStart + indent.length };
  }

  // 光标在行内
  const lineEnd = lineRightEdge(text, c, lineStart);
  const line = text.slice(lineStart, lineEnd);
  const leading = line.length - line.trimStart().length;
  const trimmed = line.slice(leading);

  const curChain = orderedChainOf(trimmed);
  if (curChain) {
    // 有序列表：缩进一层（降级）。新链规则：上一有序行 P 是当前链 C 的真前缀
    // → 新链 = C + [1]；否则 → P + [1]
    const markerLen = listMarkerOf(trimmed)?.length ?? 0;
    const parent = findPrevOrderedChain(text, lineStart);
    let newChain: number[];
    if (parent && isPrefixOf(parent, curChain)) {
      newChain = [...curChain, 1];
    } else if (parent) {
      newChain = [...parent, 1];
    } else {
      return { text, cursor: c }; // 前面没有有序项：无效果
    }
    const indent = "  ".repeat(newChain.length - 1);
    const newMarker = `${indent}${formatChain(newChain)}`;
    const contentOffset = Math.max(0, c - (lineStart + leading + markerLen));
    const newText = `${text.slice(0, lineStart)}${newMarker}${text.slice(lineStart + leading + markerLen)}`;
    return { text: newText, cursor: lineStart + newMarker.length + contentOffset };
  }

  // 普通/无序/任务行：行首插 2 空格
  return { text: `${text.slice(0, lineStart)}  ${text.slice(lineStart)}`, cursor: c + 2 };
}

/**
 * Shift+Tab：有序提升一级（目标编号 = 该层已有项数 + 1）；无序/任务去掉 2 空格缩进。
 * 围栏内/顶层有序/普通行 → null（默认行为）。
 */
export function handleShiftTabAtCursor(text: string, cursor: number): EditResult | null {
  const c = Math.min(cursor, text.length);
  const lineStart = text.lastIndexOf("\n", c - 1) + 1;
  if (inFenceAt(text, lineStart)) return null;
  const lineEnd = lineRightEdge(text, c, lineStart);
  const line = text.slice(lineStart, lineEnd);
  const leading = line.length - line.trimStart().length;
  const trimmed = line.slice(leading);

  const chain = orderedChainOf(trimmed);
  if (chain) {
    if (chain.length === 1) return null; // 已是顶层
    const markerLen = listMarkerOf(trimmed)?.length ?? 0;
    const targetLen = chain.length - 1;
    const prefix = targetLen >= 2 ? chain.slice(0, targetLen - 1) : [];
    const newChain = [...prefix, countChainRows(text, prefix, targetLen) + 1];
    const indent = "  ".repeat(newChain.length - 1);
    const newMarker = `${indent}${formatChain(newChain)}`;
    const contentOffset = Math.max(0, c - (lineStart + leading + markerLen));
    const newText = `${text.slice(0, lineStart)}${newMarker}${text.slice(lineStart + leading + markerLen)}`;
    return { text: newText, cursor: lineStart + newMarker.length + contentOffset };
  }

  // 无序/任务列表：去掉前 2 空格缩进
  if (leading >= 2 && listMarkerOf(trimmed) !== null) {
    return { text: `${text.slice(0, lineStart)}${text.slice(lineStart + 2)}`, cursor: Math.max(0, c - 2) };
  }
  return null;
}
