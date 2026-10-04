// Tauri invoke / listen 封装
import { convertFileSrc as tauriConvertFileSrc, invoke as tauriInvoke } from "@tauri-apps/api/core";
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

/**
 * 本地文件路径 → WebView 可加载的 `asset:` URL（便签里的图片显示用）。
 *
 * 需要 tauri.conf.json 打开 `app.security.assetProtocol`（已开启，scope 覆盖工作空间）。
 * 测试/非 Tauri 环境下内部实现不存在，调用方需自行 try/catch（见 `utils/asset-url.ts`）。
 */
export function convertFileSrc(path: string): string {
  return tauriConvertFileSrc(path);
}
