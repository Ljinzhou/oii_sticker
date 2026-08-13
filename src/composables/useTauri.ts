// Tauri invoke / listen 封装
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen, type UnlistenFn } from "@tauri-apps/api/event";

/** 调用 Rust 命令（参数与 Rust 侧 snake_case 一致）。 */
export function invoke<T = unknown>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return tauriInvoke<T>(cmd, args);
}

/** 监听后端事件，返回取消监听函数。 */
export function listen<T = unknown>(
  event: string,
  handler: (payload: T) => void,
): Promise<UnlistenFn> {
  return tauriListen<T>(event, (e) => handler(e.payload));
}
