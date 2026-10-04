// 便签标题约定：取便签内容中「第一个一级标题（# ）」，主控台与编辑器共用。
// 一级标题可出现在任意行（例如第一行是正文、第二行才是 # 标题，则标题取第二行）。

/** 只包含一张图片的行（粘贴图片生成的 `![alt](path)`）：推导标题时要跳过，
 *  否则「第一行是图片、没有一级标题」的便签会把图片路径当标题（文件名也会跟着变脏）。 */
const IMAGE_ONLY_LINE = /^!\[[^\]]*\]\([^)]*\)$/;

/** 从内容提取标题：第一个 `# ` 一级标题；没有则退回首个非空行（截断 30 字符，跳过纯图片行）。 */
export function extractTitleFromContent(content: string): string {
  for (const line of content.split("\n")) {
    const trimmed = line.trim();
    if (trimmed.startsWith("# ")) {
      return trimmed.slice(2).trim();
    }
  }
  const first = content
    .split("\n")
    .find((l) => {
      const trimmed = l.trim();
      return trimmed.length > 0 && !IMAGE_ONLY_LINE.test(trimmed);
    }) ?? "";
  return first.trim().slice(0, 30);
}

/** 将内容中第一个一级标题行替换为新标题；内容中没有一级标题时在开头插入 `# newTitle`。
 *  用于主控台重命名：改标题必须同步改便签内容，否则下次保存会被内容重新推导覆盖。 */
export function setTitleInContent(content: string, newTitle: string): string {
  const lines = content.split("\n");
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].trim().startsWith("# ")) {
      lines[i] = `# ${newTitle}`;
      return lines.join("\n");
    }
  }
  return `# ${newTitle}\n\n${content}`;
}
