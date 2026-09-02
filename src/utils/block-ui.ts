// Todo 块 / 已完成任务块的「用户界面状态」持久化。
//
// 设计约束（重要）：折叠/展开、子任务显隐、已完成来源选择，全部由用户操作
// 驱动；程序只读写这份状态，绝不自动改变它。状态存 system_config 的
// todo_block_ui 条目（JSON），跨窗口、重启后保持。
//
// 键约定：
//  - folds[rootId]        卡片整体折叠（todo-block 卡头下拉箭头）
//  - subs[taskId]         某任务的子任务隐藏（任务行下拉箭头）
//  - doneFolds[stickerId] 已完成任务卡折叠
//  - doneSrc[stickerId]   已完成任务卡的数据来源根任务 id（"" = 全部）
import { useSettingsStore } from "../stores/settings";

export interface BlockUiState {
  folds: Record<string, boolean>;
  subs: Record<string, boolean>;
  doneFolds: Record<string, boolean>;
  doneSrc: Record<string, string>;
}

export const DEFAULT_BLOCK_UI: BlockUiState = {
  folds: {},
  subs: {},
  doneFolds: {},
  doneSrc: {},
};

export const BLOCK_UI_KEY = "todo_block_ui";

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** 宽松解析：任意脏数据都回退到默认结构，渲染层永不崩。 */
export function parseBlockUi(raw: unknown): BlockUiState {
  if (!isRecord(raw)) return { ...DEFAULT_BLOCK_UI };
  const pick = (key: string): Record<string, boolean | string> =>
    isRecord(raw[key]) ? (raw[key] as Record<string, boolean | string>) : {};
  const normBool = (input: Record<string, boolean | string>): Record<string, boolean> => {
    const out: Record<string, boolean> = {};
    for (const [k, v] of Object.entries(input)) if (typeof v === "boolean") out[k] = v;
    return out;
  };
  const normStr = (input: Record<string, boolean | string>): Record<string, string> => {
    const out: Record<string, string> = {};
    for (const [k, v] of Object.entries(input)) if (typeof v === "string") out[k] = v;
    return out;
  };
  return {
    folds: normBool(pick("folds")),
    subs: normBool(pick("subs")),
    doneFolds: normBool(pick("doneFolds")),
    doneSrc: normStr(pick("doneSrc")),
  };
}

/** 从 settings store 快照读取当前 UI 状态（渲染路径用，纯同步）。 */
export function readBlockUi(): BlockUiState {
  try {
    const settings = useSettingsStore();
    return parseBlockUi(safeJsonParse(settings.config.entries[BLOCK_UI_KEY]));
  } catch {
    // 非应用上下文（单测等）没有 Pinia：返回默认值即可
    return { ...DEFAULT_BLOCK_UI };
  }
}

function safeJsonParse(raw: string | undefined): unknown {
  if (!raw) return undefined;
  try {
    return JSON.parse(raw) as unknown;
  } catch {
    return undefined;
  }
}

export type BlockUiAction = "foldCard" | "foldSub" | "doneFold" | "doneSource";

/**
 * 用户操作入口：切换/写入对应键并持久化，返回更新后的快照。
 * 只在事件处理器里调用（需要活跃的 Pinia）。
 */
export async function applyBlockUiAction(
  action: BlockUiAction,
  id: string,
  value?: string,
): Promise<BlockUiState> {
  const settings = useSettingsStore();
  const current = readBlockUi();
  const next: BlockUiState = {
    folds: { ...current.folds },
    subs: { ...current.subs },
    doneFolds: { ...current.doneFolds },
    doneSrc: { ...current.doneSrc },
  };
  switch (action) {
    case "foldCard":
      next.folds[id] = !(current.folds[id] ?? false);
      break;
    case "foldSub":
      next.subs[id] = !(current.subs[id] ?? false);
      break;
    case "doneFold":
      next.doneFolds[id] = !(current.doneFolds[id] ?? false);
      break;
    case "doneSource":
      next.doneSrc[id] = value ?? "";
      break;
  }
  await settings.set(BLOCK_UI_KEY, JSON.stringify(next));
  return next;
}
