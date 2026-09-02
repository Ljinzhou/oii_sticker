// 外部内容 → CodeMirror 编辑器的安全同步（Markdown 源码模式 / 及时预览共用）。
//
// 旧实现是「全量替换」：dispatch({ changes: { from: 0, to: doc.length, insert: doc } })。
// 全量替换有两个严重副作用：
//  1) 光标瞬移：CodeMirror 把选区按 change 映射，整篇被替换时光标会被折叠到
//     替换起点（位置 0），表现为连续退格时"光标跳到文件最开头"；
//  2) 打断输入法：中文组词（composition）期间重设文档会强行结束组词，
//     已删除/未上屏的字符可能"复活"，删不干净。
//
// 这里改为：
//  - 计算新旧文本的最小差异（公共前缀 + 公共后缀），只替换差异段，
//    选区在差异段之外时 CodeMirror 原样保留光标位置；
//  - 组词期间（view.composing）直接跳过外部同步——此时几乎必然是自己
//    回写的 echo，真正的外部更新等组词结束后的下一次同步自然生效。
import type { EditorView } from "@codemirror/view";

/** 用最小差异把外部文本同步进编辑器；内容相同或正在组词时跳过。 */
export function applyExternalDoc(view: EditorView, doc: string): void {
  const current = view.state.doc.toString();
  if (current === doc) return;
  // 输入法组词期间绝不重设文档（打断组词 = 字符复活/光标乱跳）
  if (view.composing) return;

  // 公共前缀
  let start = 0;
  const minLen = Math.min(current.length, doc.length);
  while (start < minLen && current.charCodeAt(start) === doc.charCodeAt(start)) start++;
  // 公共后缀（不越过前缀）
  let endCurrent = current.length;
  let endNext = doc.length;
  while (
    endCurrent > start &&
    endNext > start &&
    current.charCodeAt(endCurrent - 1) === doc.charCodeAt(endNext - 1)
  ) {
    endCurrent--;
    endNext--;
  }

  view.dispatch({
    changes: { from: start, to: endCurrent, insert: doc.slice(start, endNext) },
  });
}
