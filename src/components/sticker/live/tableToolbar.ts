// 表格浮动工具条（Obsidian / Notion 风格）：
// 光标 / 选中单元格位于表格内时浮现在表格上方；每个按钮直接改写 Markdown 源码。
// 采用纯 DOM 实现（与 liveWidgets 同思路）：位置依赖编辑器布局，由 LiveEditorView 驱动更新。
import {
  tableActionDisabled,
  type TableAlign,
  type TableCellFormat,
  type TableCellRange,
  type TableToolbarAction,
  type TableToolbarState,
} from "./liveTableEdits";

/* ── 图标：统一「表格网格 + 高亮带 + 操作符号」底座，保证按钮视觉一致 ── */
const ROW_TOP = [2.2, 6.06, 9.93];
const ROW_H = 3.86;
const COL_X = [1.2, 6];
const COL_W = 4.8;

type Glyph = "plus" | "up" | "down" | "left" | "right" | "cross";

function glyph(kind: Glyph, color: string): string {
  const stroke = `stroke="${color}" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" fill="none"`;
  switch (kind) {
    case "plus":
      return `<path ${stroke} d="M12.42 8h4.16M14.5 5.92v4.16"/>`;
    case "up":
      return `<path ${stroke} d="M14.5 11.4V4.9M12.85 6.6 14.5 4.75 16.15 6.6"/>`;
    case "down":
      return `<path ${stroke} d="M14.5 4.6v6.5M12.85 9.4 14.5 11.25 16.15 9.4"/>`;
    case "left":
      return `<path ${stroke} d="M16.58 8h-6.5M13.85 6.35 12 8l1.85 1.65"/>`;
    case "right":
      return `<path ${stroke} d="M12.42 8h6.5M15.15 6.35 17 8l-1.85 1.65"/>`;
    case "cross":
      return `<path ${stroke} d="M12.7 6.2 16.3 9.8M16.3 6.2 12.7 9.8"/>`;
  }
}

interface GridIcon {
  row?: number;
  col?: number;
  symbol: Glyph;
  danger?: boolean;
}

function gridIcon({ row, col, symbol, danger }: GridIcon): string {
  const tone = danger ? "var(--tbl-danger, #b42318)" : "var(--tbl-accent, #4f7cff)";
  const band = danger ? "var(--tbl-danger-band, rgba(180,35,24,.16))" : "var(--tbl-accent-band, rgba(79,124,255,.22))";
  const highlight = row !== undefined
    ? `<rect x="1.2" y="${ROW_TOP[row]}" width="9.6" height="${ROW_H}" fill="${band}"/>`
    : `<rect x="${COL_X[col ?? 0]}" y="2.2" width="${COL_W}" height="11.6" fill="${band}"/>`;
  return `<svg viewBox="0 0 18 16" aria-hidden="true">
    <g stroke="currentColor" stroke-width="1.15" fill="none" opacity="0.62">
      <rect x="1.2" y="2.2" width="9.6" height="11.6" rx="1.4"/>
      <path d="M1.2 6.06h9.6M1.2 9.93h9.6M6 2.2v11.6"/>
    </g>
    ${highlight}
    ${glyph(symbol, tone)}
  </svg>`;
}

function alignIcon(mode: TableAlign): string {
  const stroke = `stroke="currentColor" stroke-width="1.6" stroke-linecap="round"`;
  if (mode === "default") {
    return `<svg viewBox="0 0 16 16" aria-hidden="true"><path ${stroke} stroke-dasharray="2.5 2.1" d="M2.4 8h11.2"/></svg>`;
  }
  const lines: Array<[number, number]> = [[3.4, 12.6], [5.6, 10.4], [4.1, 11.9]];
  const y = [4.2, 8, 12.0];
  const paths = lines
    .map(([from, to], index) => {
      const length = to - from;
      const start = mode === "left" ? 2.4 : mode === "right" ? 13.6 - length : 8.5 - length / 2;
      return `M${start.toFixed(2)} ${y[index]}h${length}`;
    })
    .join("");
  return `<svg viewBox="0 0 16 16" aria-hidden="true"><path ${stroke} d="${paths}"/></svg>`;
}

/** 单元格格式按钮图标：字母字形（删除线额外压一条横线）。 */
function formatIcon(format: TableCellFormat): string {
  const letter = format === "bold" ? "B" : format === "italic" ? "I" : "S";
  const weight = format === "bold" ? 800 : 650;
  const skew = format === "italic" ? ` transform="skewX(-12)"` : "";
  const overline = format === "strike"
    ? `<path stroke="currentColor" stroke-width="1.4" stroke-linecap="round" d="M3.6 8h8.8"/>`
    : "";
  return `<svg viewBox="0 0 16 16" aria-hidden="true">
    <text x="8" y="8.4" text-anchor="middle" dominant-baseline="central" font-size="11.5" font-weight="${weight}"
      font-family="Georgia, 'Times New Roman', serif" fill="currentColor"${skew}>${letter}</text>
    ${overline}
  </svg>`;
}

interface ToolbarButton {
  act: TableToolbarAction;
  label: string;
  icon: () => string;
  danger?: boolean;
}

/** 按钮顺序即工具条顺序；组间以分隔线区隔（格式 | 对齐 | 行 | 列）。 */
const GROUPS: ToolbarButton[][] = [
  [
    { act: "bold", label: "加粗 (Ctrl+B)", icon: () => formatIcon("bold") },
    { act: "italic", label: "斜体 (Ctrl+I)", icon: () => formatIcon("italic") },
    { act: "strike", label: "删除线 (Ctrl+Shift+X)", icon: () => formatIcon("strike") },
  ],
  [
    { act: "align-left", label: "左对齐", icon: () => alignIcon("left") },
    { act: "align-center", label: "居中对齐", icon: () => alignIcon("center") },
    { act: "align-right", label: "右对齐", icon: () => alignIcon("right") },
    { act: "align-default", label: "默认对齐（清除）", icon: () => alignIcon("default") },
  ],
  [
    { act: "row-above", label: "在上方插入行", icon: () => gridIcon({ row: 0, symbol: "plus" }) },
    { act: "row-below", label: "在下方插入行", icon: () => gridIcon({ row: 2, symbol: "plus" }) },
    { act: "row-up", label: "上移当前行", icon: () => gridIcon({ row: 1, symbol: "up" }) },
    { act: "row-down", label: "下移当前行", icon: () => gridIcon({ row: 1, symbol: "down" }) },
    { act: "row-delete", label: "删除当前行", icon: () => gridIcon({ row: 1, symbol: "cross", danger: true }), danger: true },
  ],
  [
    { act: "col-insert-left", label: "在左侧插入列", icon: () => gridIcon({ col: 0, symbol: "plus" }) },
    { act: "col-insert-right", label: "在右侧插入列", icon: () => gridIcon({ col: 1, symbol: "plus" }) },
    { act: "col-move-left", label: "左移当前列", icon: () => gridIcon({ col: 0, symbol: "left" }) },
    { act: "col-move-right", label: "右移当前列", icon: () => gridIcon({ col: 1, symbol: "right" }) },
    { act: "col-delete", label: "删除当前列", icon: () => gridIcon({ col: 0, symbol: "cross", danger: true }), danger: true },
  ],
];

export interface TableToolbarAnchor {
  left: number;
  top: number;
}

export class TableToolbar {
  readonly dom: HTMLDivElement;

  /** 当前工具条作用的表格在文档中的位置（表格内任意偏移，由调用方在同步时更新）。
   *  不能依赖编辑器光标：点击单元格不会移动编辑器选区。 */
  tablePos = -1;

  /** 当前选中的单元格区域（同步时写入；动作按该区域生效）。 */
  range: TableCellRange | null = null;

  private readonly buttons = new Map<TableToolbarAction, HTMLButtonElement>();

  constructor(private readonly onAction: (action: TableToolbarAction) => void) {
    this.dom = document.createElement("div");
    this.dom.className = "tbl-bar";
    this.dom.hidden = true;
    this.dom.setAttribute("role", "toolbar");
    this.dom.setAttribute("aria-label", "表格工具");
    // 工具条自身不参与编辑器选区/输入：按下即阻止默认，保持单元格焦点不变
    this.dom.addEventListener("mousedown", (event) => event.preventDefault());

    GROUPS.forEach((group, index) => {
      if (index > 0) {
        const sep = document.createElement("i");
        sep.className = "tbl-sep";
        this.dom.append(sep);
      }
      for (const item of group) {
        const button = document.createElement("button");
        button.type = "button";
        button.className = `tbl-btn${item.danger ? " is-danger" : ""}`;
        button.dataset.act = item.act;
        button.dataset.tip = item.label;
        button.setAttribute("aria-label", item.label);
        button.innerHTML = item.icon();
        button.addEventListener("click", (event) => {
          event.preventDefault();
          event.stopPropagation();
          if (button.disabled) return;
          this.onAction(item.act);
        });
        this.buttons.set(item.act, button);
        this.dom.append(button);
      }
    });
  }

  /** 更新按钮状态与浮层位置；state 为 null 表示目标不在表格内 → 隐藏。
   *  tablePos 传入时同步更新（表格内稳定位置，动作据此定位）。 */
  update(
    state: TableToolbarState | null,
    anchor: TableToolbarAnchor | null,
    tablePos?: number,
  ): void {
    if (!state) {
      this.hide();
      this.tablePos = -1;
      this.range = null;
      return;
    }
    if (tablePos !== undefined) this.tablePos = tablePos;
    this.range = state.range;
    for (const [action, button] of this.buttons) {
      button.disabled = tableActionDisabled(action, state);
      if (action.startsWith("align-")) {
        button.classList.toggle("is-on", state.align === action.slice("align-".length));
      } else if (action === "bold" || action === "italic" || action === "strike") {
        button.classList.toggle("is-on", state.formats[action]);
      }
    }
    if (anchor) {
      this.dom.style.left = `${Math.round(anchor.left)}px`;
      this.dom.style.top = `${Math.round(anchor.top)}px`;
    }
    this.dom.hidden = false;
  }

  hide(): void {
    this.dom.hidden = true;
  }

  /** 按钮数量（测试与调试用）。 */
  get size(): number {
    return this.buttons.size;
  }
}
