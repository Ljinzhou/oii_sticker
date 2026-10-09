//! 名称规则：便签标题与分组名共用的一套**系统级**限制。
//!
//! 便签标题即 md 文件名、分组名即文件夹名（见 `layout`/`commands`），
//! 因此命名必须与资源管理器口径一致，用户手工建文件时不能做的，程序里也不能做：
//!
//! - 不允许 `< > : " / \ | ? *` 与控制字符；
//! - 不允许以句点结尾（末尾空格在 trim 时已被吃掉）；
//! - 不允许 `CON` / `PRN` / `AUX` / `NUL` / `COM1`-`COM9` / `LPT1`-`LPT9`
//!   （大小写不敏感，带扩展名的 `con.md` 同样保留）；
//! - 单个名称分量不超过 255 个 UTF-16 码元（NTFS 限制）；
//! - 拼出的完整路径不超过 259 个码元（Windows `MAX_PATH`）。
//!
//! 前端 `src/utils/file-name-rules.ts` 是同一套规则的镜像实现（即时提示用），
//! 两边的错误文案保持一字不差，用户看到的消息不因校验发生在哪一侧而不同。

use anyhow::{bail, Result};
use std::path::Path;

/// Windows 文件名禁用字符。
const FORBIDDEN: [char; 9] = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
/// 禁用字符的用户可见提示（与 [`FORBIDDEN`] 一一对应）。
const FORBIDDEN_HINT: &str = "< > : \" / \\ | ? *";
/// 保留设备名（大小写不敏感；带任意扩展名同样保留）。
const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];
/// 单个名称分量上限（UTF-16 码元，NTFS 限制）。
pub const MAX_COMPONENT_UNITS: usize = 255;
/// 完整路径上限（Windows `MAX_PATH` 含结尾 NUL 为 260）。
pub const MAX_PATH_UNITS: usize = 259;

/// 便签标题的报错前缀。
pub const STICKER_LABEL: &str = "便签名称";
/// 分组名的报错前缀。
pub const GROUP_LABEL: &str = "分组名称";

/// 是否是系统禁用字符（路径逐段校验用）。
pub fn is_forbidden_char(c: char) -> bool {
    FORBIDDEN.contains(&c)
}

/// 是否是系统保留名称（`con` / `con.txt` / `LPT1 ` 都算）。
pub fn is_reserved(raw: &str) -> bool {
    let stem = raw
        .split('.')
        .next()
        .unwrap_or(raw)
        .trim_end_matches([' ', '.']);
    RESERVED.contains(&stem.to_ascii_uppercase().as_str())
}

/// 校验名称；通过时返回去掉首尾空白后的名称。
///
/// `label` 只用于错误文案（[`STICKER_LABEL`] / [`GROUP_LABEL`]）。
pub fn validate(label: &str, raw: &str) -> Result<String> {
    let name = raw.trim();
    if name.is_empty() {
        bail!("{label}不能为空");
    }
    if name.chars().any(|c| c.is_control()) {
        bail!("{label}不能包含控制字符");
    }
    if name.chars().any(|c| FORBIDDEN.contains(&c)) {
        bail!("{label}不能包含 {FORBIDDEN_HINT} 之一");
    }
    if name.ends_with('.') {
        bail!("{label}不能以句点结尾");
    }
    if name.encode_utf16().count() > MAX_COMPONENT_UNITS {
        bail!("{label}过长（最多 {MAX_COMPONENT_UNITS} 个字符）");
    }
    if is_reserved(name) {
        bail!("「{name}」是系统保留名称，不能用作{label}");
    }
    Ok(name.to_string())
}

/// 名称是否合法（前端/调用方只需布尔判断时用）。
pub fn is_legal(raw: &str) -> bool {
    validate(STICKER_LABEL, raw).is_ok()
}

/// 校验便签标题并返回其 md 文件名（标题 + `.md`）。
pub fn sticker_md_name(title: &str) -> Result<String> {
    let name = validate(STICKER_LABEL, title)?;
    let file = format!("{name}.md");
    if file.encode_utf16().count() > MAX_COMPONENT_UNITS {
        bail!("{STICKER_LABEL}过长（最多 {} 个字符）", MAX_COMPONENT_UNITS - 3);
    }
    Ok(file)
}

/// 磁盘上的路径是否超出 Windows `MAX_PATH`。
///
/// 便签现在按 `stickers/<分组…>/<标题>.md` 存放，分组层级 + 长标题容易顶到上限，
/// 写入前先拦下来，避免只留下一个含混的系统报错。
pub fn ensure_path_fits(path: &Path) -> Result<()> {
    let units = path.to_string_lossy().encode_utf16().count();
    if units > MAX_PATH_UNITS {
        bail!(
            "路径过长（{units} > {MAX_PATH_UNITS} 个字符），请把便签放到层级更浅的分组里：{}",
            path.display()
        );
    }
    Ok(())
}

/// 把任意文本清洗成**合法且可用**的文件名（迁移 / 兜底用，绝不报错）。
///
/// 与 [`validate`] 的区别：校验是「拒绝」，清洗是「尽力救回来」——
/// 只在两个地方用：① 旧数据迁移时把历史标题洗成合法名称；
/// ② 外部文件/目录名已经存在（系统已保证合法）时的防御性归一。
pub fn sanitize_legacy(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.trim().chars() {
        out.push(if ch.is_control() || FORBIDDEN.contains(&ch) { '-' } else { ch });
    }
    let mut out = out.trim().trim_end_matches(['.', ' ']).trim().to_string();
    if out.is_empty() || out.chars().all(|c| c == '.') {
        out = "未命名".to_string();
    }
    if is_reserved(&out) {
        out.push('_');
    }
    // 为 ".md" 留出余量，避免分量顶到 255
    const KEEP: usize = MAX_COMPONENT_UNITS - 8;
    if out.encode_utf16().count() > KEEP {
        let mut cut = String::new();
        let mut units = 0usize;
        for ch in out.chars() {
            let w = ch.len_utf16();
            if units + w > KEEP {
                break;
            }
            units += w;
            cut.push(ch);
        }
        out = cut;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_and_forbidden_chars() {
        assert!(validate(STICKER_LABEL, "").is_err());
        assert!(validate(STICKER_LABEL, "   ").is_err());
        for bad in ["a/b", "a\\b", "a:b", "a*b", "a?b", "a\"b", "a<b", "a>b", "a|b"] {
            let err = validate(STICKER_LABEL, bad).unwrap_err().to_string();
            assert!(err.contains("不能包含"), "{bad} 应被拒绝：{err}");
        }
        // 控制字符（含换行/制表）
        assert!(validate(GROUP_LABEL, "a\nb").is_err());
        assert!(validate(GROUP_LABEL, "a\tb").is_err());
    }

    #[test]
    fn trims_and_keeps_chinese() {
        assert_eq!(validate(STICKER_LABEL, "  随手记  ").unwrap(), "随手记");
        assert_eq!(validate(STICKER_LABEL, "oii_sticker Bug记录").unwrap(), "oii_sticker Bug记录");
        assert_eq!(validate(STICKER_LABEL, "带.句点.的").unwrap(), "带.句点.的");
    }

    #[test]
    fn rejects_trailing_dot_and_reserved_names() {
        let err = validate(STICKER_LABEL, "名字.").unwrap_err().to_string();
        assert!(err.contains("句点"), "{err}");
        for bad in ["CON", "con", "Con.md", "nul", "LPT1", "com9.txt", "AUX "] {
            let err = validate(GROUP_LABEL, bad).unwrap_err().to_string();
            assert!(err.contains("保留名称"), "{bad} 应被拒绝：{err}");
        }
        // 只是以保留名开头则没问题
        assert!(validate(GROUP_LABEL, "console").is_ok());
        assert!(validate(GROUP_LABEL, "COM10").is_ok());
    }

    #[test]
    fn rejects_overlong_component() {
        // 255 个 UTF-16 码元是 Windows 单个名称分量的上限：正好 255 可以用，超过拒绝
        let long = "字".repeat(MAX_COMPONENT_UNITS + 1);
        assert!(validate(STICKER_LABEL, &long).is_err());
        let ok = "字".repeat(MAX_COMPONENT_UNITS);
        assert!(validate(STICKER_LABEL, &ok).is_ok());
    }

    #[test]
    fn sticker_md_name_appends_suffix_and_validates() {
        assert_eq!(sticker_md_name("欢迎使用").unwrap(), "欢迎使用.md");
        assert!(sticker_md_name("非法/名字").is_err());
        // 255 - 3 = 252 个字是文件名上限
        let ok = "字".repeat(252);
        assert!(sticker_md_name(&ok).is_ok());
        assert!(sticker_md_name(&"字".repeat(253)).is_err());
    }

    #[test]
    fn sanitize_legacy_rescues_bad_text() {
        // 只替换系统真正非法的字符：全角「？」在 Windows 文件名里合法，保持原样
        assert_eq!(sanitize_legacy("我的 笔记/啊？"), "我的 笔记-啊？");
        assert_eq!(sanitize_legacy(".."), "未命名");
        assert_eq!(sanitize_legacy("  "), "未命名");
        assert_eq!(sanitize_legacy("名字."), "名字");
        assert_eq!(sanitize_legacy("CON"), "CON_");
        assert_eq!(sanitize_legacy("a\u{0}b"), "a-b");
    }

    #[test]
    fn path_length_guard() {
        let ok = Path::new("C:\\ws\\stickers\\随手记\\便签.md");
        assert!(ensure_path_fits(ok).is_ok());
        let long = format!("C:\\ws\\{}.md", "a".repeat(400));
        assert!(ensure_path_fits(Path::new(&long)).is_err());
    }
}
