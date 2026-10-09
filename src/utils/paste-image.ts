// 便签编辑器「粘贴图片」：把剪贴板里的图片交给后端落盘，并在光标处插入 Markdown。
//
// 约定（与后端一致）：图片落在
// `<工作空间>/assets/<分组路径>/<便签标题>_<便签 id>/粘贴-YYYYMMDD-HHmmss.<ext>`，
// md 里插入的是**相对工作空间**的路径 `![粘贴图片](assets/…)`，
// 显示时由 `utils/asset-url.ts` 解析成 asset: URL；便签改名/换分组时后端会
// 搬动该目录并改写正文里的链接。

import type { EditorView } from "@codemirror/view";
import { invoke } from "../composables/useTauri";

/** 粘出来的图片统一用这个 alt 文本。 */
export const PASTED_IMAGE_ALT = "粘贴图片";

/** MIME → 扩展名（后端只认白名单里的扩展名）。 */
const EXT_BY_MIME: Record<string, string> = {
  "image/png": "png",
  "image/jpeg": "jpg",
  "image/jpg": "jpg",
  "image/webp": "webp",
  "image/gif": "gif",
  "image/bmp": "bmp",
};

const EXT_BY_NAME = /\.(png|jpe?g|webp|gif|bmp)$/i;

/** 从剪贴板数据里挑出图片文件；没有图片返回空数组。 */
export function imageFilesFrom(data: DataTransfer | null | undefined): File[] {
  if (!data) return [];
  const files: File[] = [];
  // 1) 文件列表（拖拽/复制文件、部分系统的截图粘贴）
  for (const file of Array.from(data.files ?? [])) {
    if (file.type.startsWith("image/")) files.push(file);
  }
  if (files.length > 0) return files;
  // 2) items（Windows 截图工具/浏览器复制图片走这里，files 可能为空）
  for (const item of Array.from(data.items ?? [])) {
    if (item.kind !== "file" || !item.type.startsWith("image/")) continue;
    const file = item.getAsFile();
    if (file) files.push(file);
  }
  return files;
}

/** 图片扩展名：MIME 优先，其次文件名后缀，最后兜底 png。 */
export function imageExt(file: File): string {
  const byMime = EXT_BY_MIME[file.type.toLowerCase()];
  if (byMime) return byMime;
  const byName = EXT_BY_NAME.exec(file.name)?.[1]?.toLowerCase();
  if (byName) return byName === "jpeg" ? "jpg" : byName;
  return "png";
}

/** Uint8Array → base64（分块避免超长参数把调用栈撑爆）。 */
export function bytesToBase64(bytes: Uint8Array): string {
  const chunk = 0x8000;
  let binary = "";
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}

/** 落盘一张图片，返回 md 里可用的相对路径。 */
export async function savePastedImage(stickerId: number, file: File): Promise<string> {
  const bytes = new Uint8Array(await file.arrayBuffer());
  return await invoke<string>("save_sticker_image_cmd", {
    stickerId,
    ext: imageExt(file),
    dataBase64: bytesToBase64(bytes),
  });
}

/** 便签图片的 Markdown 文本。 */
export function imageMarkdown(relPath: string): string {
  return `![${PASTED_IMAGE_ALT}](${relPath})`;
}

/**
 * 让插入片段**独占一行**：图片 Markdown 若直接贴到相邻文字上，
 * 会和那一行粘成一句（例如插到 `# 标题` 前面变成 `![..](..)# 标题`，
 * 标题行就失效了，便签标题还会被推导成图片路径）。
 *
 * @returns 实际插入文本与插入后的光标位置。
 */
export function standaloneInsertion(
  doc: string,
  pos: number,
  text: string,
): { insert: string; anchor: number } {
  const before = pos > 0 ? doc.slice(pos - 1, pos) : "";
  const after = pos < doc.length ? doc.slice(pos, pos + 1) : "";
  const prefix = before && before !== "\n" ? "\n" : "";
  const suffix = after && after !== "\n" ? "\n" : "";
  const insert = `${prefix}${text}${suffix}`;
  return { insert, anchor: pos + insert.length };
}

/**
 * 处理一次粘贴事件。
 *
 * 剪贴板里没有图片时**什么都不做**（不 preventDefault）：纯文本/链接粘贴
 * （含 `LiveEditorView` 的「粘贴链接自动转 Markdown」）保持原有行为。
 * 有图片时阻止默认粘贴（否则可能插入空内容），逐张落盘并按顺序插入到光标处。
 *
 * @returns 实际插入的 Markdown 片段（便于调用方/测试断言）；没有图片返回 `null`。
 */
export async function handleImagePaste(
  event: ClipboardEvent,
  view: EditorView,
  stickerId: number,
): Promise<string | null> {
  const files = imageFilesFrom(event.clipboardData);
  if (files.length === 0) return null;
  event.preventDefault();
  const inserted: string[] = [];
  for (const file of files) {
    const rel = await savePastedImage(stickerId, file);
    const text = imageMarkdown(rel);
    const doc = view.state.doc.toString();
    const pos = view.state.selection.main.from;
    const { insert, anchor } = standaloneInsertion(doc, pos, text);
    view.dispatch({
      changes: { from: pos, insert },
      selection: { anchor },
    });
    inserted.push(text);
  }
  view.focus();
  return inserted.join("\n");
}

/**
 * 给 CodeMirror 视图挂上图片粘贴处理（Live 及时预览 / Markdown 源码两种内核共用）。
 * 监听器挂在 `view.dom` 上，视图销毁时随 DOM 一起移除。
 */
export function attachImagePaste(
  view: EditorView,
  stickerId: number,
  onError?: (message: string) => void,
): void {
  // 兜底：单测里的编辑器内核替身可能没有 dom（真实 EditorView 一定有）
  const dom = (view as { dom?: HTMLElement } | null)?.dom;
  if (!dom?.addEventListener) return;
  dom.addEventListener("paste", (event) => {
    void handleImagePaste(event, view, stickerId).catch((error: unknown) => {
      const message = error instanceof Error ? error.message : String(error);
      console.error("[编辑器] 粘贴图片失败：", error);
      onError?.(`粘贴图片失败：${message}`);
    });
  });
}
