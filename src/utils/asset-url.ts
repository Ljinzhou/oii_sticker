// 便签里的本地图片路径解析。
//
// 约定：md 里存**相对工作空间**的路径（`assets/随手记/想法_7/粘贴-20261002-154500.png`），
// 这样 md 保持可移植（换机器、用别的编辑器打开都还是相对路径）；
// 显示时由应用把工作空间根目录拼上，再用 Tauri 的 `asset:` 协议交给 WebView 加载。

import { convertFileSrc, invoke } from "../composables/useTauri";

/** 工作空间根目录缓存：`undefined` = 尚未读取，`null` = 读取失败/无工作空间。 */
let cachedRoot: string | null | undefined;

/** 当前工作空间根目录（模块级缓存，避免每张图都问一次后端）。 */
export async function workspaceRoot(): Promise<string | null> {
  if (cachedRoot !== undefined) return cachedRoot;
  try {
    const entry = await invoke<{ path?: string } | null>("workspace_current_cmd");
    cachedRoot = entry?.path ?? null;
  } catch {
    cachedRoot = null;
  }
  return cachedRoot;
}

/** 清缓存（切换/恢复工作空间后调用）。 */
export function resetWorkspaceRootCache(): void {
  cachedRoot = undefined;
}

/**
 * 是否「相对本地路径」。以下都**不算**（原样交给 WebView）：
 * `http(s):` / `data:` / `blob:` / `asset:` 等带协议的 URL、`//host/x`、
 * 以 `/` 或 `\` 开头的绝对路径、`C:\...` 这类盘符路径。
 */
export function isRelativeLocalPath(src: string): boolean {
  const s = src.trim();
  if (!s) return false;
  return !/^([a-zA-Z][\w+.-]*:|\/\/|[/\\])/.test(s);
}

/** 相对路径拼到工作空间根目录下（Windows 分隔符）。 */
export function joinLocalPath(root: string, rel: string): string {
  const base = root.replace(/[\\/]+$/, "");
  const parts = rel.replace(/\\/g, "/").split("/").filter((p) => p && p !== ".");
  return [base, ...parts].join("\\");
}

/**
 * 把 md 里的图片 src 解析成 WebView 可加载的 URL：
 * 相对路径 → `asset://...`；其它（网络/绝对/data）原样返回；
 * 拿不到工作空间或环境不支持时也原样返回（宁可不显示，也不要抛异常打断渲染）。
 */
export async function resolveAssetSrc(src: string): Promise<string> {
  if (!isRelativeLocalPath(src)) return src;
  const root = await workspaceRoot();
  if (!root) return src;
  let rel = src;
  try {
    rel = decodeURIComponent(src); // markdown-it 会把空格等编码成 %20
  } catch {
    /* 非法编码：按原样使用 */
  }
  try {
    return convertFileSrc(joinLocalPath(root, rel));
  } catch {
    return src;
  }
}

/** 渲染后把所有相对路径的 `<img>` 换成 asset URL（就地替换 src）。 */
export async function resolveRelativeImages(container: HTMLElement | null | undefined): Promise<void> {
  const images = container?.querySelectorAll<HTMLImageElement>("img[src]");
  if (!images || images.length === 0) return;
  await Promise.all(
    Array.from(images).map(async (img) => {
      const src = img.getAttribute("src") ?? "";
      if (!isRelativeLocalPath(src)) return;
      const resolved = await resolveAssetSrc(src);
      if (resolved && resolved !== src) img.setAttribute("src", resolved);
    }),
  );
}
