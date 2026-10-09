// 名称规则（Rust 侧 src-tauri/src/workspace/naming.rs 的镜像实现，即时提示用）。
// 便签标题即磁盘上的 md 文件名、分组名即文件夹名，所以规则必须与资源管理器口径一致：
// 用户手工建不出来的名字，程序里也不能建（否则同步线程会反过来改便签）。
// 两边的错误文案保持一字不差，消息不因校验发生在哪一侧而不同。

/** Windows 文件名禁用字符。 */
export const FORBIDDEN_CHARS = ["<", ">", ":", '"', "/", "\\", "|", "?", "*"] as const;

/** 禁用字符的用户可见提示（与 FORBIDDEN_CHARS 一一对应）。 */
const FORBIDDEN_HINT = '< > : " / \\ | ? *';

/** 保留设备名（大小写不敏感；带任意扩展名同样保留）。 */
const RESERVED = [
  "CON", "PRN", "AUX", "NUL",
  "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
  "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/** 单个名称分量上限（UTF-16 码元，NTFS 限制）。 */
export const MAX_COMPONENT_UNITS = 255;

/** 拼出的完整路径上限（Windows MAX_PATH 含结尾 NUL 为 260）。 */
export const MAX_PATH_UNITS = 259;

/** 便签标题的报错前缀。 */
export const STICKER_LABEL = "便签名称";

/** 分组名（文件夹名）的报错前缀。 */
export const GROUP_LABEL = "分组名称";

const CONTROL_CHAR = /\p{Cc}/u;

/** 是否是系统禁用字符。 */
export function isForbiddenChar(c: string): boolean {
  return (FORBIDDEN_CHARS as readonly string[]).includes(c);
}

/** 是否是系统保留名称（con / con.txt / "LPT1 " 都算）。 */
export function isReservedName(raw: string): boolean {
  const stem = raw.split(".")[0]!.replace(/[ .]+$/, "");
  return RESERVED.includes(stem.toUpperCase());
}

/** 校验名称；通过时返回去掉首尾空白后的名称，否则返回中文原因。 */
export function validateName(
  label: string,
  raw: string,
): { ok: true; name: string } | { ok: false; error: string } {
  const name = raw.trim();
  if (!name) return { ok: false, error: `${label}不能为空` };
  if (CONTROL_CHAR.test(name)) return { ok: false, error: `${label}不能包含控制字符` };
  if ([...name].some(isForbiddenChar)) {
    return { ok: false, error: `${label}不能包含 ${FORBIDDEN_HINT} 之一` };
  }
  if (name.endsWith(".")) return { ok: false, error: `${label}不能以句点结尾` };
  // .length 就是 UTF-16 码元数，与 Rust 侧 encode_utf16().count() 同口径
  if (name.length > MAX_COMPONENT_UNITS) {
    return { ok: false, error: `${label}过长（最多 ${MAX_COMPONENT_UNITS} 个字符）` };
  }
  if (isReservedName(name)) {
    return { ok: false, error: `「${name}」是系统保留名称，不能用作${label}` };
  }
  return { ok: true, name };
}

/** 名称是否合法。 */
export function isValidName(label: string, raw: string): boolean {
  return validateName(label, raw).ok;
}

/** 校验失败时返回中文原因，通过则返回 null（表单里 `if (msg) 提示` 用）。 */
export function nameError(label: string, raw: string): string | null {
  const r = validateName(label, raw);
  return r.ok ? null : r.error;
}

/** 便签标题长度上限：文件名还要加 `.md`（Rust 侧 sticker_md_name 同此口径）。 */
export function stickerTitleError(raw: string): string | null {
  const r = validateName(STICKER_LABEL, raw);
  if (!r.ok) return r.error;
  if (`${r.name}.md`.length > MAX_COMPONENT_UNITS) {
    return `${STICKER_LABEL}过长（最多 ${MAX_COMPONENT_UNITS - 3} 个字符）`;
  }
  return null;
}

/**
 * 在已知同层/同分组名称里取一个不撞车的名字：`标题` → `标题 2` → `标题 3`…
 * 比较忽略大小写（Windows 文件名不区分大小写），与后端 ensure_title_free 同一口径。
 */
export function uniqueName(existing: string[], wanted: string, max = 999): string {
  const used = new Set(existing.map((n) => n.toLowerCase()));
  if (!used.has(wanted.toLowerCase())) return wanted;
  for (let i = 2; i <= max; i++) {
    const candidate = `${wanted} ${i}`;
    if (!used.has(candidate.toLowerCase())) return candidate;
  }
  return `${wanted} ${Date.now()}`;
}

/**
 * 把任意文本清洗成合法文件名（外部内容派生标题的兜底，绝不报错）。
 * 与 Rust 侧 naming::sanitize_legacy 同一口径：只替换系统真正非法的字符。
 */
export function sanitizeLegacyName(raw: string): string {
  let out = [...raw.trim()]
    .map((c) => (CONTROL_CHAR.test(c) || isForbiddenChar(c) ? "-" : c))
    .join("")
    .trim()
    .replace(/[. ]+$/, "")
    .trim();
  if (!out || /^\.+$/.test(out)) out = "未命名";
  if (isReservedName(out)) out += "_";
  const keep = MAX_COMPONENT_UNITS - 8; // 给 ".md" 留余量
  if (out.length > keep) {
    // 按 UTF-16 码元截断，且不切断代理对（与 Rust 侧逐 char 累加同口径）
    let cut = "";
    for (const ch of out) {
      if (cut.length + ch.length > keep) break;
      cut += ch;
    }
    out = cut;
  }
  return out;
}
