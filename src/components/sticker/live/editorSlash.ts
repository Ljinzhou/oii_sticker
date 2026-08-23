// 斜杠菜单定位与查询上报（及时预览 / Markdown 两种编辑模式共用同一份实现）。
import type { EditorView } from "@codemirror/view";
import type { SlashAnchor } from "../../slash/types";

/** 光标位置 → 斜杠菜单锚点。 */
export function slashAnchorAtSelection(view: EditorView): SlashAnchor {
  const coords = view.coordsAtPos(view.state.selection.main.head);
  const host = view.dom.getBoundingClientRect();
  if (!coords) return { left: 6, top: 30 };
  return {
    left: Math.max(0, coords.left - host.left),
    top: Math.max(0, coords.bottom - host.top + 6),
  };
}

/** 检测光标所在行的 / 查询并上报（无匹配时关闭菜单）。 */
export function reportSlash(
  view: EditorView,
  onSlash: (query: string, from: number, to: number, anchor: SlashAnchor) => void,
  onSlashClose?: () => void,
): void {
  const doc = view.state.doc.toString();
  const head = view.state.selection.main.head;
  const match = /(?:^|\n)\/([^\s\/]*)$/.exec(doc.slice(0, head));
  if (!match) {
    onSlashClose?.();
    return;
  }
  const from = head - match[0].length + (match[0].startsWith("\n") ? 1 : 0);
  onSlash(match[1], from, head, slashAnchorAtSelection(view));
}
