// Todo 提醒触发事件的共享监听（todo://reminder-fired）。
//
// 后端 reminder.rs 调度线程到点后广播该事件（所有窗口都收到）：
// - 便签窗口按 sticker_id 过滤，只响应自己的任务；
// - Todo 窗口按所属 sticker 过滤；
// - 收到后各窗口自行刷新块数据（高亮由 reminded_at / due_notified_at
//   驱动渲染）并弹应用内提示。系统通知由后端回调直接发出，不经此处。
import { listen } from "../composables/useTauri";
import type { UnlistenFn } from "@tauri-apps/api/event";

/** todo://reminder-fired 事件负载（对应 Rust FireContext 序列化）。 */
export interface TodoReminderPayload {
  id: string;
  sticker_id: number;
  title: string;
  block_title: string;
  /** "reminder" = 到点提醒；"due" = 截止提醒。 */
  kind: "reminder" | "due";
  scheduled_at: string;
}

/** 提示文案：「 提醒：买菜」/「 截止：交报告」。 */
export function reminderToastText(payload: TodoReminderPayload): string {
  const name = payload.title.trim() || payload.block_title.trim() || "未命名任务";
  return ` ${payload.kind === "due" ? "截止" : "提醒"}：${name}`;
}

/**
 * 监听提醒触发事件。
 *
 * @param opts.stickerId 只响应该便签的任务（缺省 = 全部）
 * @param opts.onFire 命中回调（已通过 stickerId 过滤）
 * @returns 取消监听函数
 */
export async function watchTodoReminders(opts: {
  stickerId?: number;
  onFire: (payload: TodoReminderPayload) => void;
}): Promise<UnlistenFn> {
  return listen<TodoReminderPayload>("todo://reminder-fired", (payload) => {
    if (!payload || typeof payload.id !== "string") return;
    if (opts.stickerId !== undefined && payload.sticker_id !== opts.stickerId) return;
    opts.onFire(payload);
  });
}
