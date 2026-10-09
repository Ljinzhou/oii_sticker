//! 工作空间文件系统的外部改动同步。
//!
//! 磁盘布局即数据结构：`stickers/<分组路径>/<标题>.md` 是一张便签，
//! 目录树就是分组树，`assets/<分组路径>/<标题>_<id>/` 是它的附件。
//! 用户完全可以直接用资源管理器/编辑器增删改这些文件，程序必须立刻跟上：
//!
//! - 新建文件（可选带新文件夹）→ 建出同名分组并导入便签；
//! - 改内容 → 同步 DB 回退副本（**不回写文件**，文件始终是主存储）；
//! - 改文件名 / 改文件夹名 → 同步 DB 的标题与分组（**md 文件不搬动**，用户改名即意图），
//!   但附件目录 `assets/<分组>/<标题>_<id>/` 会跟着搬走并改写正文里的图片链接；
//! - 删除文件 → 删除对应便签（连带附件目录），但**必须先"见过"这个文件**。
//!
//! 「见过才删」是这里最关键的安全约定：DB 里有便签而文件不在，可能是
//! ①用户刚删掉文件（要删便签），也可能是 ②旧版本数据/工作空间被整体搬走（绝不能删数据）。
//! 二者无法从磁盘状态区分，因此引入进程内的 `seen` 集合：
//! 只有本次运行中确实见过该便签的文件、随后文件消失，才判定为「用户删除」；
//! 其余情况一律**用 DB 回退副本补齐 md 文件**（迁移 / 自愈），绝不删除数据。
//!
//! 同理，`stickers/` 整个目录读不到时直接放弃本轮同步（可能是盘符没挂上）。
//!
//! `sync_once` 是幂等的纯 DB+文件操作（不依赖 watcher），由后台线程在目录快照变化时调用，
//! 因此切换工作空间、程序重启、漏事件都能自愈。

use anyhow::Result;
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::layout::Layout;
use super::md_store;
use super::naming;
use crate::commands;
use crate::db::group_repo;
use crate::db::sticker_repo::{self, NewSticker, StickerPatch};
use crate::models::{Sticker, StickerGroup};

/// 一次同步的变更集合（供事件推送）。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SyncReport {
    /// 外部新增并导入的便签 id。
    pub imported: Vec<i64>,
    /// 外部修改（内容 / 标题 / 位置）后同步到 DB 的便签 id。
    pub updated: Vec<i64>,
    /// 文件被外部删除而移除的便签 id。
    pub removed: Vec<i64>,
    /// 缺文件、用 DB 回退副本补齐了 md 的便签 id（迁移 / 自愈）。
    pub restored: Vec<i64>,
    /// 因磁盘目录而新建或改名的分组 id（前端需整体刷新分组树）。
    pub groups: Vec<i64>,
}

impl SyncReport {
    pub fn is_empty(&self) -> bool {
        self.imported.is_empty()
            && self.updated.is_empty()
            && self.removed.is_empty()
            && self.restored.is_empty()
            && self.groups.is_empty()
    }

    /// 所有受影响的便签 id（去重，供前端刷新）。
    pub fn touched(&self) -> Vec<i64> {
        let mut ids: Vec<i64> = self
            .imported
            .iter()
            .chain(self.updated.iter())
            .chain(self.removed.iter())
            .chain(self.restored.iter())
            .copied()
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }
}

/// 目录快照项：相对路径 + 修改时间（秒）+ 大小；用于判断"是否值得跑一次同步"。
/// 目录本身也进快照（增删文件夹同样要触发同步），其大小记为 0。
pub type Snapshot = Vec<(String, u64, u64)>;

/// 递归扫描 `stickers/` 得到轻量快照（目录不存在时为空）。
///
/// 隐藏项（`.` 开头，含备份 staging 残留）不参与同步：它们不是用户内容。
/// 例外是点开头的 `.md`：Windows 允许 `.秘密.md` 这种合法文件名，而便签标题以句点开头时
/// 程序自己就会写出这种文件——跳过它，这张便签就永远看不见自己的 md（改名、外部编辑全丢）。
pub fn snapshot(db_path: &str) -> Snapshot {
    let dir = Layout::at(&commands::ws_root(db_path)).stickers_dir();
    let mut out: Snapshot = Vec::new();
    let mut stack: Vec<(PathBuf, String)> = vec![(dir, String::new())];
    while let Some((path, rel)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') && !is_md(&name) {
                continue;
            }
            let Ok(kind) = entry.file_type() else { continue };
            let child = rel_join(&rel, &name);
            if kind.is_dir() {
                out.push((child.clone(), mtime(&entry), 0));
                stack.push((entry.path(), child));
            } else if kind.is_file() {
                if !is_md(&name) {
                    continue;
                }
                let len = entry.metadata().map(|m| m.len()).unwrap_or(0);
                out.push((child, mtime(&entry), len));
            }
        }
    }
    out.sort();
    out
}

fn mtime(entry: &std::fs::DirEntry) -> u64 {
    entry
        .metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn is_md(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".md")
}

fn rel_join(dir: &str, leaf: &str) -> String {
    if dir.is_empty() {
        leaf.to_string()
    } else {
        format!("{dir}/{leaf}")
    }
}

/// 拆出「父目录相对路径 / 最后一段」。
fn split_last(rel: &str) -> (&str, &str) {
    match rel.rfind('/') {
        Some(i) => (&rel[..i], &rel[i + 1..]),
        None => ("", rel),
    }
}

/// 磁盘上的一张便签文件。
#[derive(Debug, Clone)]
struct DiskFile {
    /// 相对 `stickers/` 的完整路径（如 `随手记/想法.md`）。
    rel: String,
    /// 所在目录（相对 `stickers/`，根目录为 `""`）。
    dir_rel: String,
    /// 去掉 `.md` 的文件名（即便签标题）。
    title: String,
    path: PathBuf,
}

/// 递归收集 `stickers/` 下的 md 文件与全部子目录（隐藏项跳过，不跟随符号链接）。
///
/// 隐藏项里的点开头 `.md` 例外（见 [`snapshot`] 的说明）：标题以句点开头时合法的便签文件
/// 就是这种名字，必须收进来，否则它永远对不上账。
fn walk(
    dir: &Path,
    rel_prefix: &str,
    files: &mut Vec<DiskFile>,
    dirs: &mut Vec<String>,
    depth: usize,
) {
    if depth > 64 {
        tracing::warn!("[同步] 目录层数过深，已停止深入：{}", dir.display());
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') && !is_md(&name) {
            continue;
        }
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            let rel = rel_join(rel_prefix, &name);
            dirs.push(rel.clone());
            walk(&entry.path(), &rel, files, dirs, depth + 1);
        } else if kind.is_file() && is_md(&name) {
            let stem = name[..name.len() - 3].trim().to_string();
            if stem.is_empty() {
                continue;
            }
            files.push(DiskFile {
                rel: rel_join(rel_prefix, &name),
                dir_rel: rel_prefix.to_string(),
                title: stem,
                path: entry.path(),
            });
        }
    }
}

/// 某个目录里所有便签的标题集合（小写、去重、排序；用于识别"文件夹改名"）。
fn dir_titles(files: &[DiskFile], rel: &str) -> Vec<String> {
    let mut v: Vec<String> = files
        .iter()
        .filter(|f| f.dir_rel == rel)
        .map(|f| f.title.to_lowercase())
        .collect();
    v.sort();
    v.dedup();
    v
}

/// 某个分组现有便签的标题集合（同 `dir_titles` 的口径）。
fn group_titles(rows: &[Sticker], gid: i64) -> Vec<String> {
    let mut v: Vec<String> = rows
        .iter()
        .filter(|r| r.group_id == Some(gid))
        .map(|r| r.title.to_lowercase())
        .collect();
    v.sort();
    v.dedup();
    v
}

/// 把磁盘目录映射到分组：目录名即分组名（以磁盘为准），缺失的分组自动建出。
///
/// 三种情形：
/// 1. 同名分组已存在 → 直接对应（大小写不一致时按磁盘名改正）；
/// 2. 目录名在库里没有，但某个**目录已消失**的分组，其便签标题集合与这个目录完全一致
///    → 认定用户在资源管理器里给文件夹改了名（或搬了位置），改库里的名字/父级，
///    保留分组 id（颜色、顺序、便签归属都不丢）；
/// 3. 其余 → 新建同名分组。
fn resolve_dirs(
    conn: &Connection,
    dirs: &[String],
    files: &[DiskFile],
    rows: &[Sticker],
    groups: &mut Vec<StickerGroup>,
    report: &mut SyncReport,
) -> Result<HashMap<String, i64>> {
    let mut map: HashMap<String, i64> = HashMap::new();
    // 每个已有分组"应当"在的目录（据此判断它的目录是否还在磁盘上）
    let mut dir_of: HashMap<i64, String> = HashMap::new();
    for g in groups.iter() {
        dir_of.insert(g.id, commands::group_rel_path(conn, Some(g.id))?);
    }
    let present: HashSet<&str> = dirs.iter().map(|s| s.as_str()).collect();

    for rel in dirs {
        let (parent_rel, name) = split_last(rel);
        let parent_id = if parent_rel.is_empty() {
            None
        } else {
            map.get(parent_rel).copied()
        };

        // 1) 同级同名分组
        let mut exact = true;
        let mut found = groups
            .iter()
            .find(|g| g.parent_id == parent_id && g.name.eq_ignore_ascii_case(name))
            .cloned();
        if found.is_none() {
            // 旧版允许在分组名里写非法字符，文件夹里会被清洗成 `-`
            // （「电子/嵌入式学习」→ 目录「电子-嵌入式学习」）：认得出这个目录是它的，
            // 但**不**把分组名改成清洗后的形式——那是用户的原始名字。
            exact = false;
            found = groups
                .iter()
                .find(|g| {
                    g.parent_id == parent_id
                        && naming::sanitize_legacy(&g.name).eq_ignore_ascii_case(name)
                })
                .cloned();
        }
        if let Some(g) = found {
            if exact && g.name != name {
                // 磁盘是主存储：文件夹名怎么写的，分组名就怎么走
                group_repo::rename(conn, g.id, name)?;
                if let Some(slot) = groups.iter_mut().find(|x| x.id == g.id) {
                    slot.name = name.to_string();
                }
                report.groups.push(g.id);
                tracing::info!("[同步] 分组 {} 按磁盘目录改名为「{name}」", g.id);
            }
            map.insert(rel.clone(), g.id);
            continue;
        }

        // 2) 目录已消失、便签标题集合一致的分组 = 被改了名的那个分组
        //    （空目录不参与：空集合会跟任何空分组"相符"，一个随手建的
        //    空文件夹就能把别的分组连 id 带颜色一起认走）
        let want = dir_titles(files, rel);
        let mut candidates: Vec<StickerGroup> = if want.is_empty() {
            Vec::new()
        } else {
            groups
                .iter()
                .filter(|g| {
                    !map.values().any(|v| *v == g.id)
                        && !present.contains(dir_of.get(&g.id).map(|s| s.as_str()).unwrap_or(""))
                        && group_titles(rows, g.id) == want
                })
                .cloned()
                .collect()
        };
        if candidates.len() == 1 {
            let g = candidates.remove(0);
            let moved = g.parent_id != parent_id;
            if moved {
                group_repo::move_to_parent(conn, g.id, parent_id)?;
            }
            group_repo::rename(conn, g.id, name)?;
            if let Some(slot) = groups.iter_mut().find(|x| x.id == g.id) {
                slot.name = name.to_string();
                slot.parent_id = parent_id;
            }
            report.groups.push(g.id);
            tracing::info!("[同步] 目录 {rel} 认领了原分组 {}（{}{}）", g.id, g.name, if moved { "，并已移动父级" } else { "" });
            map.insert(rel.clone(), g.id);
            continue;
        }

        // 3) 新建分组
        let created = group_repo::create(conn, name, parent_id)?;
        report.groups.push(created.id);
        tracing::info!("[同步] 外部目录 {rel} → 新建分组「{name}」（id={}）", created.id);
        map.insert(rel.clone(), created.id);
        groups.push(created);
    }
    Ok(map)
}

/// 文件名里的旧 `{id}-` 前缀不算标题：DB 标题与它一致时以 DB 为准，
/// 免得在一个尚未迁移的工作空间里把标题改成 "14-标题"。
fn effective_title(stem: &str, row: &Sticker) -> String {
    if let Some((head, rest)) = stem.split_once('-') {
        if !head.is_empty()
            && head.chars().all(|c| c.is_ascii_digit())
            && rest.trim() == row.title
        {
            return row.title.clone();
        }
    }
    stem.to_string()
}

/// 组内重名时的兜底标题（同目录不能有两个同名 md）。
fn unique_title(conn: &Connection, group_id: Option<i64>, title: &str) -> Result<String> {
    if sticker_repo::find_by_title_in_group(conn, group_id, title, None)?.is_none() {
        return Ok(title.to_string());
    }
    for n in 2..1000 {
        let candidate = format!("{title} {n}");
        if sticker_repo::find_by_title_in_group(conn, group_id, &candidate, None)?.is_none() {
            return Ok(candidate);
        }
    }
    Ok(format!("{title} {}", std::process::id()))
}

/// 把磁盘文件与 DB 便签配对。
///
/// 返回 `(配对, 未配对文件下标, 未配对便签下标)`，配对项是
/// `(便签下标, 文件下标, 位置以谁为准)`：第三个值为 `true` 表示"文件还停在程序上次
/// 记录的位置（或旧版 `{id}-标题.md` 命名）"，也就是用户没动过它——此时位置由 DB 说了算，
/// 顺手把它搬进新布局（分组子目录 + 去掉 id 前缀），分组不跟着路径走。
///
/// 配对依次尝试：`file_name` 缓存精确命中 → 旧版 id 前缀命名 → 缓存路径改名/跨目录搬动
/// → 同分组同标题 → 同分组同内容 → 同目录一对一 → 跨分组同内容且唯一。后几步是
/// "用户在资源管理器里改了文件名/拖到别的文件夹"的识别：便签不该被当成
/// "删掉一个+新加一个"，否则 id、窗口位置、任务都会跟着换一遍。
fn bind_files(
    files: &[DiskFile],
    rows: &[Sticker],
    dir_group: &HashMap<String, i64>,
    orphan_rows: &HashSet<i64>,
) -> (Vec<(usize, usize, bool)>, Vec<usize>, Vec<usize>) {
    let mut file_taken = vec![false; files.len()];
    let mut row_taken = vec![false; rows.len()];
    let mut pairs: Vec<(usize, usize, bool)> = Vec::new();
    let group_of = |f: &DiskFile| dir_group.get(&f.dir_rel).copied();
    // 用宏而非闭包：闭包同时借用两个 taken 数组会被借检查器拒绝
    macro_rules! bind {
        ($fi:expr, $ri:expr) => {{
            file_taken[$fi] = true;
            row_taken[$ri] = true;
            pairs.push(($ri, $fi, false));
        }};
    }
    macro_rules! bind_db_wins {
        ($fi:expr, $ri:expr) => {{
            file_taken[$fi] = true;
            row_taken[$ri] = true;
            pairs.push(($ri, $fi, true));
        }};
    }

    // A) DB 缓存的路径精确命中（程序自己写的名字没被用户动过）
    for (fi, f) in files.iter().enumerate() {
        let hit = rows
            .iter()
            .enumerate()
            .position(|(ri, r)| !row_taken[ri] && r.file_name.as_deref().map(str::trim) == Some(f.rel.as_str()));
        if let Some(ri) = hit {
            bind_db_wins!(fi, ri);
        }
    }

    // A0) 旧版命名 `{id}-标题.md`（改造前所有便签都平铺在 stickers/ 根目录下）：
    //     前缀里的 id 直接指名道姓，认下来后由第 4 步把它搬进新布局。
    //     不认这一步，旧工作空间一升级就会"旧便签补回分组目录 + 平铺文件被当成新便签导入"，
    //     每张便签都变两份。
    //     标题也要对得上：用户自己起的 `12-别的.md` 不能被当成 id=12 的便签
    //     （否则会顶掉它的 DB 正文、把文件搬走、连标题一起改掉）。
    for (fi, f) in files.iter().enumerate() {
        if file_taken[fi] || !f.dir_rel.is_empty() {
            continue;
        }
        let Some((id, rest)) = legacy_parts(&f.title) else { continue };
        let rest = rest.trim();
        let hit = rows.iter().enumerate().position(|(ri, r)| {
            !row_taken[ri]
                && r.id == id
                && (rest == r.title.trim() || rest == naming::sanitize_legacy(&r.title).trim())
        });
        if let Some(ri) = hit {
            bind_db_wins!(fi, ri);
        }
    }

    // A2) 缓存路径的「文件名部分」命中：便签被搬到别的文件夹（换了分组）时整条
    //     缓存路径对不上，但文件名没变——不认这一步就会变成"删一张 + 新建一张"，
    //     id、窗口位置、任务都会跟着换掉。只在候选唯一时认，避免张冠李戴。
    for (fi, f) in files.iter().enumerate() {
        if file_taken[fi] {
            continue;
        }
        let Some(leaf) = f.path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        let has_cache = rows.iter().any(|r| r.file_name.is_some());
        if !has_cache {
            continue; // 没有任何缓存信息时这一步无从判断，交给 B~D
        }
        let mut hit: Option<usize> = None;
        let mut ambiguous = false;
        for (ri, r) in rows.iter().enumerate() {
            if row_taken[ri] {
                continue;
            }
            let cached_leaf = r
                .file_name
                .as_deref()
                .map(str::trim)
                .and_then(|rel| rel.rsplit(['/', '\\']).next())
                .unwrap_or("");
            if cached_leaf.is_empty() || !cached_leaf.eq_ignore_ascii_case(&leaf) {
                continue;
            }
            if hit.is_some() {
                ambiguous = true;
                break;
            }
            hit = Some(ri);
        }
        if let Some(ri) = hit {
            if !ambiguous {
                bind!(fi, ri);
            }
        }
    }

    // B) 同分组 + 同标题（文件被改名后缓存路径过期，但标题没变）
    for (fi, f) in files.iter().enumerate() {
        if file_taken[fi] {
            continue;
        }
        let gid = group_of(f);
        let hit = rows
            .iter()
            .enumerate()
            .position(|(ri, r)| !row_taken[ri] && r.group_id == gid && r.title.eq_ignore_ascii_case(&f.title));
        if let Some(ri) = hit {
            bind!(fi, ri);
        }
    }

    // C) 同分组 + 内容完全相同（只改了文件名，正文一个字没动）
    for (fi, f) in files.iter().enumerate() {
        if file_taken[fi] {
            continue;
        }
        let gid = group_of(f);
        let Ok(content) = std::fs::read_to_string(&f.path) else { continue };
        let hit = rows
            .iter()
            .enumerate()
            .position(|(ri, r)| !row_taken[ri] && r.group_id == gid && r.content == content);
        if let Some(ri) = hit {
            bind!(fi, ri);
        }
    }

    // D) 同一个目录里各自只剩一个：视为改名（内容也改过，无法用 C 判定）。
    //
    // 分组允许放宽到 [`orphan_rows`]：那些便签上次运行还看得见文件，这次它的分组文件夹
    // 在磁盘上整个不见了（文件夹改名 + 文件改名 + 正文改动三件事一起发生时，
    // 目录↔分组的改名识别认不出来，只会在第 4 步重新建一个分组）。
    // 不放进来的话，这些便签会落进第 6 步被当成"用户删了 md"而删掉（连附件目录、任务一起），
    // 正文又以新 id 重新导入。仍然要求该目录里 free_files / free_rows 各只剩一个。
    for (fi, f) in files.iter().enumerate() {
        if file_taken[fi] {
            continue;
        }
        let free_rows: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(ri, r)| {
                !row_taken[*ri] && (r.group_id == group_of(f) || orphan_rows.contains(&r.id))
            })
            .map(|(ri, _)| ri)
            .collect();
        let free_files: Vec<usize> = files
            .iter()
            .enumerate()
            .filter(|(i, x)| !file_taken[*i] && x.dir_rel == f.dir_rel)
            .map(|(i, _)| i)
            .collect();
        if free_rows.len() == 1 && free_files.len() == 1 {
            bind!(fi, free_rows[0]);
        }
    }

    // C2) 跨分组 + 内容完全相同，且双方都唯一：一次操作里同时「改名 + 换文件夹」时
    //     前面的步骤全对不上（A2 比文件名、B/C/D 都比分组），不认就会变成
    //     "删一张 + 新建一张"。内容相同可能是巧合，所以两侧都必须唯一才认。
    for (fi, f) in files.iter().enumerate() {
        if file_taken[fi] {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&f.path) else { continue };
        let rows_hit: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(ri, r)| !row_taken[*ri] && r.content == content)
            .map(|(ri, _)| ri)
            .collect();
        if rows_hit.len() != 1 {
            continue;
        }
        let same_content_files = files
            .iter()
            .enumerate()
            .filter(|(i, x)| {
                !file_taken[*i] && std::fs::read_to_string(&x.path).is_ok_and(|c| c == content)
            })
            .count();
        if same_content_files == 1 {
            bind!(fi, rows_hit[0]);
        }
    }

    let free_files = (0..files.len()).filter(|i| !file_taken[*i]).collect();
    let free_rows = (0..rows.len()).filter(|i| !row_taken[*i]).collect();
    (pairs, free_files, free_rows)
}

/// 拆旧版文件名 `12-标题`：返回 `(id, 标题)`；不是这个形状则返回 `None`。
///
/// 只认「数字 + `-` + 非空标题」，避免把用户自己起的 `2024-计划` 误当成 id 前缀
/// （`2024` 也得真有对应便签才会配对上）。
fn legacy_parts(stem: &str) -> Option<(i64, &str)> {
    let (head, rest) = stem.split_once('-')?;
    if head.is_empty() || rest.trim().is_empty() || !head.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((head.parse::<i64>().ok()?, rest))
}

/// 这个"没配上对"的文件是不是**旧版程序留下的重复副本**？是则返回它对应的便签 id。
///
/// 旧构建（改造前）只扫 `stickers/` 一层，并按 `{id}-标题.md` "补齐"它找不到的便签
/// 文件；新布局把 md 放进了分组子目录，旧构建看不见，于是它每次启动都会在根目录
/// 重建出这些带 id 前缀的平铺副本——新旧版本混跑时，用户就会看到每张便签变成两份。
/// 这种副本是纯冗余，直接删掉，绝不能导入成新便签。
///
/// 四个条件必须同时成立（宁可漏删，不可误删）：
/// 1. 文件在 `stickers/` 根目录，且名字是旧版 `{id}-标题.md`；
/// 2. 该 id 真有便签，且文件名里的标题还对得上（用户的改名不算，那走别的配对步骤）；
/// 3. 这张便签本轮配到了**别的**文件（它的真实 md 在别处，且不是这个文件自己）；
/// 4. 两个文件内容逐字节相同。
fn stale_legacy_copy(
    root: &Path,
    f: &DiskFile,
    rows: &[Sticker],
    live_rel: &HashMap<i64, String>,
) -> Option<i64> {
    if !f.dir_rel.is_empty() {
        return None;
    }
    let (id, rest) = legacy_parts(&f.title)?;
    let row = rows.iter().find(|r| r.id == id)?;
    let rest = rest.trim();
    if rest != row.title.trim() && rest != naming::sanitize_legacy(&row.title).trim() {
        return None;
    }
    let live = live_rel.get(&id)?;
    if live == &f.rel {
        return None;
    }
    let live_path = md_store::sticker_path(root, live).ok()?;
    let stale = std::fs::read(&f.path).ok()?;
    let kept = std::fs::read(&live_path).ok()?;
    if stale != kept {
        return None;
    }
    Some(id)
}

/// 搬动附件目录（相对 `assets/`）：改名 / 换分组后让附件跟着便签走。
///
/// 目录不存在（没有附件）时静默成功；跨卷自动退化为复制+删源。
/// 用 `move_path_replacing`：目录名里带着便签 id（`<标题>_<id>`），撞名只可能是这张便签
/// 自己的旧目录，必须覆盖，否则改名会失败。
fn move_assets(root: &Path, old_rel: &str, new_rel: &str) -> Result<()> {
    let from = md_store::asset_path(root, old_rel)?;
    let to = md_store::asset_path(root, new_rel)?;
    md_store::move_path_replacing(&from, &to)
}

/// 让 DB 与 `stickers/` 目录树一致（幂等）。
///
/// `seen`：本次运行中"确认见过其文件"的便签 id 集合，由调用方（后台线程）跨轮维护；
/// 切换工作空间时必须清空（id 空间不同）。它决定"文件消失"是删除便签还是补齐文件。
pub fn sync_once(conn: &Connection, db_path: &str, seen: &mut HashSet<i64>) -> Result<SyncReport> {
    let root = commands::ws_root(db_path);
    let stickers_dir = Layout::at(&root).stickers_dir();
    let mut report = SyncReport::default();
    // 目录不存在（被移动/卸载）时不动 DB：否则会把"整个目录读不到"误判成"便签全被删了"
    if !stickers_dir.is_dir() {
        return Ok(report);
    }

    // 1) 磁盘扫描
    let mut files: Vec<DiskFile> = Vec::new();
    let mut dirs: Vec<String> = Vec::new();
    walk(&stickers_dir, "", &mut files, &mut dirs, 0);
    files.sort_by(|a, b| a.rel.cmp(&b.rel));
    dirs.sort();

    // 2) 目录 ↔ 分组
    let existing = sticker_repo::list_all(conn)?;
    // 先记录每张便签当前的附件目录：resolve_dirs 会改分组名/父级，
    // 之后再算就只剩下新路径，旧目录永远搬不动了。
    let old_assets: Vec<(i64, String)> = existing
        .iter()
        .filter_map(|s| commands::sticker_asset_dir(conn, s).ok().map(|d| (s.id, d)))
        .collect();
    let mut groups = group_repo::list(conn)?;
    let dir_group = resolve_dirs(conn, &dirs, &files, &existing, &mut groups, &mut report)?;

    // 3) 文件 ↔ 便签
    //    先算「孤儿分组便签」：上次运行还见过它的文件、这次它所属分组的文件夹在磁盘上
    //    已经不存在了（文件夹被改名/搬走，而目录↔分组的改名识别没认出来）。
    //    这些便签仍应被"同一目录里各自只剩一个"认领，否则它们会走到第 6 步的删除分支。
    //    刻意只包含 `seen` 里的便签：从没见过文件的便签照旧走第 6 步补 md，不参与认领，
    //    免得张冠李戴。
    let orphan_rows: HashSet<i64> = existing
        .iter()
        .filter(|s| seen.contains(&s.id) && s.group_id.is_some())
        .filter(|s| {
            commands::group_rel_path(conn, s.group_id)
                .map(|rel| !rel.is_empty() && !stickers_dir.join(&rel).is_dir())
                .unwrap_or(false)
        })
        .map(|s| s.id)
        .collect();
    let (pairs, free_files, free_rows) = bind_files(&files, &existing, &dir_group, &orphan_rows);

    // 4) 已配对：文件是主存储，把变化同步回 DB
    // 顺带记下每张便签本轮真正的 md 位置（下面识别旧版残留副本时要用）
    let mut live_rel: HashMap<i64, String> = HashMap::new();
    for (ri, fi, db_wins) in pairs {
        let row = &existing[ri];
        let f = &files[fi];
        seen.insert(row.id);
        // 读不到就跳过这一轮：绝不能把"临时读失败"当成"正文是空的"写回 DB（会毁掉正文）。
        // continue 发生在 `live_rel` 写入之前，于是本轮的 `stale_legacy_copy` 也不会碰这张便签。
        let Ok(mut content) = std::fs::read_to_string(&f.path) else {
            tracing::warn!("[同步] 读取 {} 失败，本轮跳过这张便签", f.rel);
            continue;
        };
        let title = effective_title(&f.title, row);
        // 位置以谁为准：用户没动过文件（缓存命中 / 旧版 id 前缀命名）就听 DB 的，
        // 否则听磁盘的（文件在哪一层目录，就属于那个分组）。
        let group_id = if db_wins {
            row.group_id
        } else {
            dir_group.get(&f.dir_rel).copied()
        };
        // 该待的位置：<分组路径>/<标题>.md。旧布局的平铺文件与带 id 前缀的文件都在这儿被搬正。
        let want_rel = commands::sticker_rel_path_for(conn, group_id, &title)?;
        // 只差大小写（`笔记.MD` vs `笔记.md`）时视为**已经就位**：Windows 上它们是同一个文件，
        // 去搬只会撞上 move_path 的"同一文件"判断，还把 DB 里的路径写成磁盘上的另一种大小写。
        if !f.rel.eq_ignore_ascii_case(&want_rel) {
            if let Err(e) = md_store::move_path(
                &md_store::sticker_path(&root, &f.rel)?,
                &md_store::sticker_path(&root, &want_rel)?,
            ) {
                tracing::warn!("[同步] 搬动 {0} → {1} 失败（{e:#}），保留原位置", f.rel, want_rel);
            }
        }
        let cur_rel = if root.join("stickers").join(&f.rel).is_file() {
            f.rel.clone()
        } else {
            want_rel.clone()
        };
        live_rel.insert(row.id, cur_rel.clone());
        // 改名或换分组 → 附件目录跟着走，正文里的图片链接同步改写，
        // 否则「资源管理器里改个文件名」就会让图片全部失效。
        let new_asset =
            commands::asset_dir_rel(&commands::group_rel_path(conn, group_id)?, &title, row.id);
        let old_asset = old_assets
            .iter()
            .find(|(id, _)| *id == row.id)
            .map(|(_, d)| d.clone());
        if let Some(old_asset) = old_asset.filter(|d| *d != new_asset) {
            match move_assets(&root, &old_asset, &new_asset) {
                Ok(()) => {
                    let rewritten = commands::rewrite_asset_links(&content, &old_asset, &new_asset);
                    if rewritten != content {
                        if let Err(e) = md_store::write(&root, &cur_rel, &rewritten) {
                            tracing::warn!("[同步] 改写图片链接失败（{}）：{e:#}", cur_rel);
                        } else {
                            content = rewritten;
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("[同步] 搬动附件目录失败（{old_asset} → {new_asset}）：{e:#}")
                }
            }
        }
        let mut moved = false;
        let mut patch = StickerPatch::default();
        if content != row.content {
            patch.content = Some(content);
        }
        if title != row.title {
            patch.title = Some(title);
        }
        if row.file_name.as_deref() != Some(cur_rel.as_str()) {
            patch.file_name = Some(cur_rel.clone());
        }
        // 文件被搬到别的分组文件夹（或从分组里搬回根目录）→ 分组跟着变
        if row.group_id != group_id {
            moved = true;
        }
        if patch.content.is_none() && patch.title.is_none() && patch.file_name.is_none() && !moved {
            continue; // 一致：无动作（幂等）
        }
        if let Some(content) = &patch.content {
            // 内容变化也要清掉"标签已无"的孤儿任务，避免 Todo 窗口继续显示
            let _ = commands::collect_orphan_blocks(conn, row.id, content);
        }
        sticker_repo::update(conn, row.id, &patch)?;
        if moved {
            sticker_repo::set_group(conn, row.id, group_id)?;
        }
        report.updated.push(row.id);
    }

    // 4.5) 旧版程序留下的重复平铺副本：`{id}-标题.md` 躺在 `stickers/` 根目录，而这张
    //      便签的真实 md 已经在别处存着内容完全相同的副本——旧构建只看得见一层目录，
    //      看不到分组子目录，于是它每次启动都会把这些文件"补齐"回来。
    //      这种副本是纯冗余，删掉即可，绝不能导入成新便签。
    //      只打日志、不进 `report.removed`：那是"用户删了便签"，会让前端关掉便签窗口。
    let mut free_files = free_files;
    for fi in free_files.clone() {
        let f = &files[fi];
        let Some(id) = stale_legacy_copy(&root, f, &existing, &live_rel) else {
            continue;
        };
        match md_store::remove(&root, &f.rel) {
            Ok(()) => {
                tracing::info!("[同步] 删除旧版残留的重复文件 {0}（便签 {id} 的 md 在别处）", f.rel);
                free_files.retain(|i| *i != fi);
            }
            Err(e) => tracing::warn!("[同步] 删除重复文件 {0} 失败：{e:#}", f.rel),
        }
    }

    // 5) 没有主人的文件：导入为新便签（**不改动磁盘**，文件名就是标题）
    for fi in free_files {
        let f = &files[fi];
        // 读不到就别导入：否则会凭空多出一张空正文的便签，下一轮又把空正文写进 md。
        let Ok(content) = std::fs::read_to_string(&f.path) else {
            tracing::warn!("[同步] 读取待导入的文件 {} 失败，跳过", f.rel);
            continue;
        };
        let title = naming::validate(naming::STICKER_LABEL, &f.title)
            .unwrap_or_else(|_| naming::sanitize_legacy(&f.title));
        let gid = dir_group.get(&f.dir_rel).copied();
        let title = unique_title(conn, gid, &title)?;
        let new = NewSticker {
            group_id: gid,
            title,
            content,
            ..Default::default()
        };
        let id = sticker_repo::insert(conn, &new)?;
        sticker_repo::update(
            conn,
            id,
            &StickerPatch {
                file_name: Some(f.rel.clone()),
                ..Default::default()
            },
        )?;
        seen.insert(id);
        tracing::info!("[同步] 导入外部便签 {}（id={id}）", f.rel);
        report.imported.push(id);
    }

    // 6) DB 有、磁盘没有：区分「用户删除」与「文件还没生成」
    for ri in free_rows {
        let row = &existing[ri];
        if seen.contains(&row.id) {
            // 本次运行见过它的文件，现在没了 → 用户手动删除 → 删便签（连带附件目录）
            commands::delete_sticker(conn, row.id, db_path)?;
            report.removed.push(row.id);
            continue;
        }
        // 从没见过文件（旧数据迁移 / 目录被整体搬走）：用 DB 回退副本补齐 md，绝不删数据
        let rel = commands::sticker_rel_path(conn, row)?;
        let path = md_store::sticker_path(&root, &rel)?;
        if path.exists() {
            continue; // 该路径已被别的文件占用：不覆盖，等下一轮再对账
        }
        match md_store::write(&root, &rel, &row.content) {
            Ok(()) => {
                seen.insert(row.id);
                tracing::info!("[同步] 便签 {} 缺 md，已按 DB 副本补齐 {rel}", row.id);
                report.restored.push(row.id);
            }
            Err(e) => tracing::warn!("[同步] 为便签 {} 补齐 md 失败：{e}", row.id),
        }
    }

    Ok(report)
}

/// 后台线程：目录快照变化 → 同步一次 → 推送刷新事件。
/// 首次进入循环先跑一次（程序启动 / 切换工作空间后立即对齐）。
pub fn spawn(app: tauri::AppHandle, state: crate::state::AppState) {
    tracing::info!("[同步] 启动工作空间文件同步线程（快照轮询）");
    let _ = std::thread::Builder::new()
        .name("workspace-sync".into())
        .spawn(move || run_loop(app, state));
}

const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(1200);

fn run_loop(app: tauri::AppHandle, state: crate::state::AppState) {
    let mut last: Option<(String, Snapshot)> = None;
    // 本次运行见过的便签文件（决定"文件消失"是删除还是补齐）；切换工作空间时清空。
    let mut seen: HashSet<i64> = HashSet::new();
    loop {
        let db_path = state.db_path();
        if db_path.is_empty() {
            std::thread::sleep(POLL_INTERVAL);
            continue;
        }
        let snap = snapshot(&db_path);
        let switched = matches!(&last, Some((path, _)) if path != &db_path);
        if switched {
            seen.clear(); // 不同工作空间的 id 空间不同，不能沿用"见过"记录
        }
        let changed = match &last {
            Some((path, prev)) => path != &db_path || prev != &snap,
            None => true,
        };
        if changed {
            last = Some((db_path.clone(), snap));
            match state.with_conn(|conn| sync_once(conn, &db_path, &mut seen)) {
                Ok(report) => {
                    if !report.is_empty() {
                        tracing::info!(
                            "[同步] 外部改动：新增 {:?} / 更新 {:?} / 补齐 {:?} / 删除 {:?} / 分组 {:?}",
                            report.imported,
                            report.updated,
                            report.restored,
                            report.removed,
                            report.groups
                        );
                        for id in report.touched() {
                            crate::events::emit_push_update(&app, id);
                        }
                        // 分组树有变 → 通知主控台整体刷新
                        if !report.groups.is_empty() {
                            crate::events::emit_push_update(&app, 0);
                        }
                        // 被外部删除的便签：把它的窗口一并关掉
                        // （前端拿到空数据只会变成空白窗，留在桌面上反而困惑）
                        {
                            use tauri::Manager;
                            for id in &report.removed {
                                if let Some(win) = app.get_webview_window(&format!("sticker-{id}")) {
                                    let _ = win.close();
                                    tracing::info!("[同步] 便签 {id} 的文件已不存在，关闭其窗口");
                                }
                            }
                            // 便签已从库里删除：通知它的任务（Todo 编辑）窗口自动保存并关闭。
                            // 必须显式发——父便签窗口可能本来就没开（用户先隐藏了便签，
                            // 再从任务总览打开任务窗口），那就没有 Destroyed 事件可依赖。
                            for id in &report.removed {
                                crate::events::emit_sticker_closed(&app, *id);
                            }
                        }
                    }
                }
                Err(e) => tracing::warn!("[同步] 失败：{e}"),
            }
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// 立即同步一次（给命令层留的入口：切换工作空间后、手动刷新）。
///
/// 注意 `seen` 的语义：它必须是**同一个工作空间**持续维护的那一份（后台线程 `run_loop`
/// 自己持有一份局部集合，且切换工作空间时会清空）。单独 new 一个空集合调用它，等于
/// "所有便签都没见过文件"——那时同步只会补齐 md，不会删任何东西（安全方向），
/// 但也就失去了"用户删了 md ⇒ 删便签"的能力。
pub fn sync_now(state: &crate::state::AppState, seen: &mut HashSet<i64>) -> Result<SyncReport> {
    let db_path = state.db_path();
    if db_path.is_empty() {
        return Ok(SyncReport::default());
    }
    state.with_conn(|conn| sync_once(conn, &db_path, seen))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::connection::open;

    struct Env {
        dir: PathBuf,
        db_path: String,
        conn: Connection,
        layout: Layout,
        seen: HashSet<i64>,
    }

    impl Drop for Env {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn env(tag: &str) -> Env {
        let dir = std::env::temp_dir().join(format!("ws-sync-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let layout = Layout::at(&dir.join("ws"));
        crate::workspace::layout::ensure_layout(&layout, "同步测试").unwrap();
        let db_path = layout.db_path().to_string_lossy().into_owned();
        let conn = open(&layout.db_path()).unwrap();
        crate::db::schema::run_migrations(&conn).unwrap();
        Env { dir, db_path, conn, layout, seen: HashSet::new() }
    }

    impl Env {
        fn sync(&mut self) -> SyncReport {
            let mut seen = std::mem::take(&mut self.seen);
            let report = sync_once(&self.conn, &self.db_path, &mut seen).unwrap();
            self.seen = seen;
            report
        }

        /// 在 stickers/ 下写一个文件（自动建出中间目录）。
        fn write(&self, rel: &str, content: &str) -> PathBuf {
            let path = self.layout.stickers_dir().join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, content).unwrap();
            path
        }

        fn md(&self, rel: &str) -> PathBuf {
            self.layout.stickers_dir().join(rel.replace('/', std::path::MAIN_SEPARATOR_STR))
        }

        fn group_id(&self, name: &str) -> i64 {
            crate::db::group_repo::list(&self.conn)
                .unwrap()
                .into_iter()
                .find(|g| g.name == name)
                .unwrap_or_else(|| panic!("分组 {name} 不存在"))
                .id
        }

        /// 建一个分组（只写 DB；文件夹由同步/命令层按需创建）。
        fn mk_group(&self, name: &str, parent: Option<i64>) -> i64 {
            crate::db::group_repo::create(&self.conn, name, parent).unwrap().id
        }
    }

    #[test]
    fn imports_file_into_new_group_created_from_folder() {
        let mut e = env("import-folder");
        e.write("随手记/想法.md", "# 外部写的");
        let report = e.sync();
        assert_eq!(report.imported.len(), 1, "{report:?}");
        let gid = e.group_id("随手记");
        assert!(report.groups.contains(&gid), "应建出同名分组：{report:?}");
        let s = sticker_repo::get(&e.conn, report.imported[0]).unwrap().unwrap();
        assert_eq!(s.title, "想法");
        assert_eq!(s.content, "# 外部写的");
        assert_eq!(s.group_id, Some(gid));
        // 文件名保持用户给的样子（不再加 id 前缀、也不改名）
        assert!(e.md("随手记/想法.md").is_file());
        assert_eq!(s.file_name.as_deref(), Some("随手记/想法.md"));
        // 幂等：再同步一次无变化
        let again = e.sync();
        assert!(again.is_empty(), "第二次应无变更：{again:?}");
    }

    #[test]
    fn empty_folder_becomes_group() {
        let mut e = env("empty-folder");
        std::fs::create_dir_all(e.md("空分组")).unwrap();
        let report = e.sync();
        assert_eq!(report.imported, Vec::<i64>::new());
        assert_eq!(report.groups.len(), 1, "{report:?}");
        assert_eq!(e.group_id("空分组"), report.groups[0]);
    }

    #[test]
    fn root_level_file_is_imported_without_group() {
        let mut e = env("root-file");
        e.write("随手记.md", "临时内容");
        let report = e.sync();
        assert_eq!(report.imported.len(), 1);
        let s = sticker_repo::get(&e.conn, report.imported[0]).unwrap().unwrap();
        assert_eq!(s.title, "随手记");
        assert_eq!(s.group_id, None);
        assert!(e.md("随手记.md").is_file(), "文件不该被改名");
        assert!(report.groups.is_empty(), "根目录不该建分组：{report:?}");
    }

    #[test]
    fn external_content_edit_updates_db_copy_not_the_file() {
        let mut e = env("content");
        e.write("学习/笔记.md", "旧内容");
        let id = e.sync().imported[0];
        e.write("学习/笔记.md", "外部改过的内容");
        let report = e.sync();
        assert_eq!(report.updated, vec![id]);
        assert_eq!(sticker_repo::get(&e.conn, id).unwrap().unwrap().content, "外部改过的内容");
        // 程序没有回写文件
        assert_eq!(std::fs::read_to_string(e.md("学习/笔记.md")).unwrap(), "外部改过的内容");
    }

    /// 用户在资源管理器里给文件改名：应当识别为「同一张便签改了标题」，
    /// 而不是「删掉一张 + 新建一张」（否则 id / 窗口位置 / 任务都会换一遍）。
    #[test]
    fn external_file_rename_updates_title_keeping_id() {
        let mut e = env("rename-file");
        e.write("学习/旧标题.md", "内容");
        let id = e.sync().imported[0];
        std::fs::rename(e.md("学习/旧标题.md"), e.md("学习/新标题.md")).unwrap();
        let report = e.sync();
        assert!(report.removed.is_empty(), "不该被当成删除：{report:?}");
        assert!(report.imported.is_empty(), "不该被当成新便签：{report:?}");
        let s = sticker_repo::get(&e.conn, id).unwrap().unwrap();
        assert_eq!(s.title, "新标题");
        assert_eq!(s.file_name.as_deref(), Some("学习/新标题.md"));
        assert!(e.md("学习/新标题.md").is_file(), "文件保持用户给的名称");
    }

    /// （内容也一起改过的）改名：同目录一对一配对兜底。
    #[test]
    fn external_file_rename_with_content_change_keeps_id() {
        let mut e = env("rename-content");
        e.write("学习/旧标题.md", "旧内容");
        let id = e.sync().imported[0];
        std::fs::rename(e.md("学习/旧标题.md"), e.md("学习/新标题.md")).unwrap();
        e.write("学习/新标题.md", "新内容");
        let report = e.sync();
        assert_eq!(report.updated, vec![id], "{report:?}");
        let s = sticker_repo::get(&e.conn, id).unwrap().unwrap();
        assert_eq!(s.title, "新标题");
        assert_eq!(s.content, "新内容");
    }

    /// 文件夹改名：认领原分组（保留分组 id 与便签归属），不新建分组。
    #[test]
    fn external_folder_rename_adopts_existing_group() {
        let mut e = env("rename-folder");
        e.write("随手记/一.md", "甲");
        e.write("随手记/二.md", "乙");
        let report = e.sync();
        let gid = e.group_id("随手记");
        assert_eq!(report.imported.len(), 2);

        std::fs::rename(e.md("随手记"), e.md("随笔")).unwrap();
        let report = e.sync();
        assert_eq!(report.groups, vec![gid], "应认领原分组：{report:?}");
        assert!(report.imported.is_empty(), "不该导入新便签：{report:?}");
        assert!(report.removed.is_empty(), "不该删除便签：{report:?}");
        let groups = crate::db::group_repo::list(&e.conn).unwrap();
        assert_eq!(groups.len(), 1, "分组数不变：{groups:?}");
        assert_eq!(groups[0].name, "随笔");
        assert_eq!(groups[0].id, gid);
        let rows = sticker_repo::list_all(&e.conn).unwrap();
        assert_eq!(rows.len(), 2);
        for r in rows {
            assert_eq!(r.group_id, Some(gid));
            assert_eq!(
                r.file_name.as_deref(),
                Some(format!("随笔/{}.md", r.title).as_str())
            );
        }
    }

    /// 文件被拖进另一个分组文件夹：便签跟着换分组。
    #[test]
    fn moving_file_between_folders_moves_sticker_group() {
        let mut e = env("move-between");
        e.write("甲组/便签.md", "内容");
        let id = e.sync().imported[0];
        std::fs::create_dir_all(e.md("乙组")).unwrap();
        std::fs::rename(e.md("甲组/便签.md"), e.md("乙组/便签.md")).unwrap();

        let report = e.sync();
        assert!(report.removed.is_empty() && report.imported.is_empty(), "{report:?}");
        let s = sticker_repo::get(&e.conn, id).unwrap().unwrap();
        assert_eq!(s.group_id, Some(e.group_id("乙组")));
        assert_eq!(s.file_name.as_deref(), Some("乙组/便签.md"));
    }

    /// 外部改名 / 拖动到别的分组：附件目录跟着便签走，正文里的图片链接同步改写。
    #[test]
    fn external_rename_moves_asset_dir_and_rewrites_links() {
        let mut e = env("rename-assets");
        e.write("随手记/旧名.md", "");
        let id = e.sync().imported[0];
        let old_assets = e.layout.assets_dir().join("随手记").join(format!("旧名_{id}"));
        std::fs::create_dir_all(&old_assets).unwrap();
        std::fs::write(old_assets.join("img.png"), b"png").unwrap();
        e.write(
            "随手记/旧名.md",
            &format!("![图](assets/随手记/旧名_{id}/img.png)"),
        );
        e.sync();

        // 一次性「改成新名 + 拖到另一个分组文件夹」
        std::fs::create_dir_all(e.md("新组")).unwrap();
        std::fs::rename(e.md("随手记/旧名.md"), e.md("新组/新名.md")).unwrap();
        let report = e.sync();
        assert!(report.removed.is_empty() && report.imported.is_empty(), "不该换 id：{report:?}");
        assert_eq!(report.updated, vec![id], "{report:?}");

        let s = sticker_repo::get(&e.conn, id).unwrap().unwrap();
        assert_eq!(s.title, "新名");
        assert_eq!(s.group_id, Some(e.group_id("新组")));
        let new_assets = e.layout.assets_dir().join("新组").join(format!("新名_{id}"));
        assert!(new_assets.join("img.png").is_file(), "附件应跟着便签走：{new_assets:?}");
        assert!(!old_assets.exists(), "旧附件目录不该留下");
        let md = std::fs::read_to_string(e.md("新组/新名.md")).unwrap();
        assert!(md.contains(&format!("assets/新组/新名_{id}/img.png")), "链接应改写：{md}");
        assert_eq!(s.content, md, "DB 副本与文件一致");
    }

    /// 只换分组不改名（拖到另一个文件夹）：同样要搬附件、留 id。
    #[test]
    fn external_move_keeps_assets_in_sync() {
        let mut e = env("move-assets");
        e.write("甲组/便签.md", "");
        let id = e.sync().imported[0];
        let old_assets = e.layout.assets_dir().join("甲组").join(format!("便签_{id}"));
        std::fs::create_dir_all(&old_assets).unwrap();
        std::fs::write(old_assets.join("a.png"), b"png").unwrap();
        e.write("甲组/便签.md", &format!("![图](assets/甲组/便签_{id}/a.png)"));
        e.sync();

        std::fs::create_dir_all(e.md("乙组")).unwrap();
        std::fs::rename(e.md("甲组/便签.md"), e.md("乙组/便签.md")).unwrap();
        let report = e.sync();
        assert_eq!(report.updated, vec![id], "{report:?}");
        assert!(e.layout.assets_dir().join("乙组").join(format!("便签_{id}")).join("a.png").is_file());
        assert!(!old_assets.exists());
    }

    #[test]
    fn deleted_file_removes_sticker_and_assets() {
        let mut e = env("delete");
        e.write("随手记/待删.md", "内容");
        let id = e.sync().imported[0];
        let gid = e.group_id("随手记");
        // 附件目录：assets/<分组>/<标题>_<id>/
        let assets = e.layout.assets_dir().join("随手记").join(format!("待删_{id}"));
        std::fs::create_dir_all(&assets).unwrap();
        std::fs::write(assets.join("img.png"), b"png").unwrap();

        std::fs::remove_file(e.md("随手记/待删.md")).unwrap();
        let report = e.sync();
        assert_eq!(report.removed, vec![id]);
        assert!(sticker_repo::get(&e.conn, id).unwrap().is_none(), "便签应被删除");
        assert!(!assets.exists(), "对应附件目录应一并清理");
        assert!(crate::db::group_repo::get(&e.conn, gid).unwrap().is_some(), "分组本身保留");
    }

    /// 关键安全语义：从没见过文件的便签（旧数据迁移 / 目录被整体搬走）不能被当成"用户删除"，
    /// 而应按 DB 回退副本补齐 md 文件。
    #[test]
    fn unseen_missing_file_is_restored_instead_of_deleted() {
        let mut e = env("restore-file");
        let id = commands::create_sticker(
            &e.conn,
            &NewSticker { title: "旧数据".into(), content: "只在 DB 里".into(), ..Default::default() },
            &e.db_path,
        )
        .unwrap();
        // 模拟"文件从未被本进程见过"（例如文件被整体搬走、或旧版本只有 DB 内容）
        std::fs::remove_file(e.md("旧数据.md")).unwrap();
        e.seen.clear();

        let report = e.sync();
        assert_eq!(report.restored, vec![id], "应补齐文件而不是删除便签：{report:?}");
        assert!(report.removed.is_empty());
        assert!(sticker_repo::get(&e.conn, id).unwrap().is_some(), "便签必须保留");
        assert_eq!(std::fs::read_to_string(e.md("旧数据.md")).unwrap(), "只在 DB 里");
        // 补齐后再次同步：稳定无变化
        assert!(e.sync().is_empty());
    }

    /// 目录整个不可读（被移走/卸载）时绝不能把 DB 便签当作"被删了"。
    #[test]
    fn missing_dir_does_not_delete_anything() {
        let mut e = env("missing-dir");
        e.write("保留.md", "内容");
        e.sync();
        std::fs::remove_dir_all(e.layout.stickers_dir()).unwrap();

        let report = e.sync();
        assert!(report.is_empty(), "不应产生任何变更：{report:?}");
        assert!(sticker_repo::get(&e.conn, 1).unwrap().is_some(), "便签必须保留");
    }

    #[test]
    fn non_markdown_files_and_hidden_dirs_are_ignored() {
        let mut e = env("other-files");
        e.write("notes.txt", "ignore me");
        e.write(".hidden/秘密.md", "不该被导入");
        e.write("附件/图.png", "png");
        let report = e.sync();
        assert!(report.imported.is_empty(), "{report:?}");
        assert!(sticker_repo::list_all(&e.conn).unwrap().is_empty());
        assert!(e.md("notes.txt").is_file(), "非 md 文件不动");
        assert!(e.md(".hidden/秘密.md").is_file(), "隐藏目录不参与同步");
    }

    /// 旧布局的 `{id}-{标题}.md`：标题不被前缀污染，文件被搬进新布局（去掉前缀）。
    #[test]
    fn legacy_id_prefixed_file_keeps_title() {
        let mut e = env("legacy-name");
        // 旧版本只有 DB 内容（`file_name` 为空），磁盘上是 `{id}-{标题}.md`
        let id = commands::create_sticker(
            &e.conn,
            &NewSticker { title: "便签".into(), content: "内容".into(), ..Default::default() },
            &e.db_path,
        )
        .unwrap();
        std::fs::rename(e.md("便签.md"), e.md(&format!("{id}-便签.md"))).unwrap();
        e.seen.clear();

        let report = e.sync();
        assert!(report.removed.is_empty(), "不该把便签当成删除：{report:?}");
        let s = sticker_repo::get(&e.conn, id).unwrap().unwrap();
        assert_eq!(s.title, "便签", "文件名里的旧 id 前缀不能变成标题");
        // 认出来之后位置由 DB 说了算：`{id}-便签.md` → `便签.md`
        assert_eq!(s.file_name.as_deref(), Some("便签.md"));
        assert!(e.md("便签.md").is_file() && !e.md(&format!("{id}-便签.md")).exists());
        assert!(e.sync().is_empty(), "第二轮应稳定：{report:?}");
    }

    /// 旧工作空间升级：`stickers/14-标题.md` 平铺文件要搬进它所属分组的目录，
    /// 分组不能跟着文件当前位置（根目录）变成"未分组"，也不能被当成新便签导入。
    #[test]
    fn legacy_flat_file_migrates_into_its_group() {
        let mut e = env("legacy-migrate");
        let g = e.mk_group("随手记", None);
        let id = commands::create_sticker(
            &e.conn,
            &NewSticker {
                title: "想法".into(),
                content: "正文".into(),
                group_id: Some(g),
                ..Default::default()
            },
            &e.db_path,
        )
        .unwrap();
        // 造出旧布局：分组目录空着，md 平铺在根目录且带 id 前缀
        std::fs::create_dir_all(e.md("随手记")).unwrap();
        std::fs::rename(e.md("随手记/想法.md"), e.md(&format!("{id}-想法.md"))).unwrap();
        e.seen.clear();

        let report = e.sync();
        assert!(report.imported.is_empty() && report.removed.is_empty(), "{report:?}");
        let s = sticker_repo::get(&e.conn, id).unwrap().unwrap();
        assert_eq!(s.group_id, Some(g), "文件位置不该把便签从分组里踢出去");
        assert_eq!(s.title, "想法");
        assert_eq!(s.file_name.as_deref(), Some("随手记/想法.md"));
        assert!(e.md("随手记/想法.md").is_file() && !e.md(&format!("{id}-想法.md")).exists());
        assert!(e.sync().is_empty(), "第二轮应稳定：{report:?}");
    }

    /// 反过来：缓存说文件在分组目录里，用户真把它拖到根目录 → 视为移出分组。
    #[test]
    fn dragging_file_to_root_ungroups_sticker() {
        let mut e = env("drag-out");
        let g = e.mk_group("随手记", None);
        let id = commands::create_sticker(
            &e.conn,
            &NewSticker {
                title: "拖出".into(),
                content: "正文".into(),
                group_id: Some(g),
                ..Default::default()
            },
            &e.db_path,
        )
        .unwrap();
        e.sync();
        std::fs::rename(e.md("随手记/拖出.md"), e.md("拖出.md")).unwrap();

        let report = e.sync();
        assert_eq!(report.updated, vec![id], "{report:?}");
        let s = sticker_repo::get(&e.conn, id).unwrap().unwrap();
        assert_eq!(s.group_id, None, "用户把文件拖到根目录＝移出分组");
        assert_eq!(s.file_name.as_deref(), Some("拖出.md"));
    }

    /// 程序自身写文件（内部保存）不应被同步误判成外部改动。
    #[test]
    fn internal_save_is_not_reported_as_change() {
        let mut e = env("internal");
        commands::create_sticker(
            &e.conn,
            &NewSticker { title: "内部".into(), content: "正文".into(), ..Default::default() },
            &e.db_path,
        )
        .unwrap();
        let report = e.sync();
        assert!(report.is_empty(), "自己写的文件不该产生同步变更：{report:?}");
    }

    #[test]
    fn snapshot_detects_nested_and_dir_changes() {
        let e = env("snap");
        assert!(snapshot(&e.db_path).is_empty());
        e.write("随手记/一.md", "x");
        let first = snapshot(&e.db_path);
        // 目录 + 文件各一条
        assert_eq!(first.len(), 2, "{first:?}");
        e.write("随手记/二.md", "y");
        assert_ne!(snapshot(&e.db_path), first, "新增文件后快照应不同");
        let before_dir = snapshot(&e.db_path);
        std::fs::create_dir_all(e.md("空目录")).unwrap();
        assert_ne!(snapshot(&e.db_path), before_dir, "新增目录后快照应不同");
    }

    #[test]
    fn report_touched_dedups_and_sorts() {
        let report = SyncReport {
            imported: vec![3, 1],
            updated: vec![1],
            removed: vec![9],
            restored: vec![5],
            groups: vec![2],
        };
        assert_eq!(report.touched(), vec![1, 3, 5, 9]);
        assert!(!report.is_empty());
        assert!(SyncReport::default().is_empty());
    }

    /// 旧版分组名里可以写非法字符（如「电子/嵌入式学习」），文件夹里是清洗后的
    /// 「电子-嵌入式学习」：同步要认得出这是同一个分组——既不能把分组名改成清洗后的
    /// 形式（那是用户的原始名字），也不能再建一个重复分组。
    #[test]
    fn legacy_group_name_with_illegal_char_maps_to_sanitized_folder() {
        let mut e = env("legacy-group-name");
        let g = crate::db::group_repo::create(&e.conn, "电子/嵌入式学习", None).unwrap().id;
        e.write("电子-嵌入式学习/笔记.md", "正文");

        let report = e.sync();
        assert!(report.groups.is_empty(), "不该改分组名，也不该建新分组：{report:?}");
        let groups = crate::db::group_repo::list(&e.conn).unwrap();
        assert_eq!(groups.len(), 1, "不能建出重复分组：{groups:?}");
        assert_eq!(groups[0].name, "电子/嵌入式学习", "分组名要保留用户的原始写法");
        let rows = sticker_repo::list_all(&e.conn).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].group_id, Some(g));
        assert_eq!(rows[0].title, "笔记");
        assert!(e.sync().is_empty(), "第二轮应稳定");
    }

    /// 旧版程序残留的重复平铺文件：这张便签的 md 已经在新布局位置（`随手记/想法.md`），
    /// 而旧构建只看得见 `stickers/` 一层，又"补齐"出一份 `{id}-想法.md`。
    /// 同步必须把它当冗余副本删掉，而不能导入成第二张便签。
    #[test]
    fn stale_legacy_flat_copy_is_deleted_not_imported() {
        let mut e = env("stale-legacy-copy");
        let g = e.mk_group("随手记", None);
        let id = commands::create_sticker(
            &e.conn,
            &NewSticker {
                group_id: Some(g),
                title: "想法".into(),
                content: "正文".into(),
                ..Default::default()
            },
            &e.db_path,
        )
        .unwrap();
        assert!(e.md("随手记/想法.md").is_file(), "新便签应落在分组目录里");
        // 旧构建"补齐"出来的副本（与真身内容逐字节相同）
        let stale = format!("{id}-想法.md");
        e.write(&stale, "正文");

        let report = e.sync();
        assert!(report.imported.is_empty(), "重复副本不该被导入：{report:?}");
        assert!(report.removed.is_empty(), "没删便签，别让前端关窗口：{report:?}");
        assert!(report.is_empty(), "{report:?}");
        assert!(!e.md(&stale).exists(), "重复副本应当被删除");
        let rows = sticker_repo::list_all(&e.conn).unwrap();
        assert_eq!(rows.len(), 1, "不能变成两张便签：{rows:?}");
        assert_eq!(rows[0].id, id);
        assert_eq!(rows[0].file_name.as_deref(), Some("随手记/想法.md"));
        assert!(e.sync().is_empty(), "第二轮应稳定");
    }

    /// 反面：根目录的 `{id}-别的名字.md` 与真身内容不同 —— 那是用户自己的文件，
    /// **不能**误删，照旧导入成新便签。
    #[test]
    fn legacy_look_alike_file_with_different_content_is_imported() {
        let mut e = env("legacy-look-alike");
        let g = e.mk_group("随手记", None);
        let id = commands::create_sticker(
            &e.conn,
            &NewSticker {
                group_id: Some(g),
                title: "想法".into(),
                content: "正文".into(),
                ..Default::default()
            },
            &e.db_path,
        )
        .unwrap();
        let other = format!("{id}-别的名字.md");
        e.write(&other, "完全不同的内容");

        let report = e.sync();
        assert_eq!(report.imported.len(), 1, "{report:?}");
        assert!(e.md(&other).exists(), "用户自己的文件不能被误删");
        assert_eq!(sticker_repo::list_all(&e.conn).unwrap().len(), 2);
    }

    /// 组合场景：文件夹改名 + 文件改名 + 正文改过，三者同时发生。
    ///
    /// 早期实现里 `resolve_dirs`「认领被改名的分组」要求标题集合完全相等，改名后集合不等
    /// 就会新建同名分组，随后所有配对步骤一起落空 → 旧便签被判成「用户删除」（连带附件、
    /// 任务一起删掉），新文件以新 id 重新导入。这里靠 D 趟对「分组目录已消失」的便签放宽
    /// 分组相等的要求，把三条改动收敛成「同一张便签」。
    #[test]
    fn folder_rename_with_file_rename_and_edit_keeps_sticker() {
        let mut e = env("combo-rename");
        let g = e.mk_group("随手记", None);
        let id = commands::create_sticker(
            &e.conn,
            &NewSticker {
                group_id: Some(g),
                title: "甲".into(),
                content: "旧正文".into(),
                ..Default::default()
            },
            &e.db_path,
        )
        .unwrap();
        e.sync();
        assert!(e.md("随手记/甲.md").is_file(), "新便签应落在分组目录里");

        // 用户在资源管理器里一次做完三件事：文件夹改名、文件改名、顺手动正文
        std::fs::rename(e.md("随手记"), e.md("随笔")).unwrap();
        std::fs::rename(e.md("随笔/甲.md"), e.md("随笔/丙.md")).unwrap();
        std::fs::write(e.md("随笔/丙.md"), "新正文").unwrap();

        let report = e.sync();
        assert!(report.removed.is_empty(), "不能删掉这张便签：{report:?}");
        assert!(report.imported.is_empty(), "也不该变成新便签：{report:?}");
        let rows = sticker_repo::list_all(&e.conn).unwrap();
        assert_eq!(rows.len(), 1, "不能变成两张：{rows:?}");
        let s = &rows[0];
        assert_eq!(s.id, id, "id 必须保住（窗口位置、任务都挂在它上面）");
        assert_eq!(s.title, "丙");
        assert_eq!(s.content, "新正文");
        assert_eq!(s.file_name.as_deref(), Some("随笔/丙.md"));
        assert_eq!(s.group_id, Some(e.group_id("随笔")), "跟着文件夹进新分组");
        let again = e.sync();
        assert!(again.is_empty(), "第二轮应稳定：{again:?}");
    }

    /// 标题以句点开头（`.秘密.md`）：能 import、能对账、且不会每轮重复动作。
    ///
    /// Windows 允许这种文件名，而便签标题以句点开头时**程序自己写出来的就是它**；
    /// 早期实现里 `walk`/`snapshot` 跳过所有 `.` 开头项，这张便签就永远读不到自己的 md。
    #[test]
    fn leading_dot_markdown_file_is_synced() {
        let mut e = env("dot-title");
        e.write(".秘密.md", "点开头");
        let report = e.sync();
        assert_eq!(report.imported.len(), 1, "{report:?}");
        let s = sticker_repo::get(&e.conn, report.imported[0]).unwrap().unwrap();
        assert_eq!(s.title, ".秘密");
        assert_eq!(s.file_name.as_deref(), Some(".秘密.md"));
        assert!(e.sync().is_empty(), "第二轮应稳定");

        // 点开头的非 md 文件仍旧忽略（`.DS_Store` 那类不是用户内容）
        e.write(".DS_Store", "x");
        let again = e.sync();
        assert!(again.imported.is_empty(), "{again:?}");
    }

    /// A0 旧版前缀认领必须核对标题：根目录的 `{id}-别人的标题.md` 是用户自己放的文件，
    /// 不能因为数字前缀撞上 id 就把真身的正文覆盖掉、把文件搬走。
    #[test]
    fn legacy_prefix_with_foreign_title_is_not_claimed() {
        let mut e = env("legacy-foreign-title");
        let g = e.mk_group("随手记", None);
        let id = commands::create_sticker(
            &e.conn,
            &NewSticker {
                group_id: Some(g),
                title: "便签".into(),
                content: "真身正文".into(),
                ..Default::default()
            },
            &e.db_path,
        )
        .unwrap();
        e.sync();
        let fake = format!("{id}-别人的标题.md");
        e.write(&fake, "用户自己写的另一份东西");

        let report = e.sync();
        assert_eq!(report.imported.len(), 1, "应作为新便签导入：{report:?}");
        assert!(report.removed.is_empty(), "{report:?}");
        let real = sticker_repo::get(&e.conn, id).unwrap().unwrap();
        assert_eq!(real.title, "便签", "真身标题不该被顶掉");
        assert_eq!(real.content, "真身正文", "真身正文不该被覆盖");
        assert!(e.md("随手记/便签.md").is_file(), "真身文件应留在分组目录里");
        assert!(e.md(&fake).exists(), "用户的文件不该被搬走或删掉");
        assert_eq!(sticker_repo::list_all(&e.conn).unwrap().len(), 2);
    }

    /// 资源管理器把 `笔记.md` 改成 `笔记.MD`（只有扩展名大小写不同）：
    /// 这就是同一个文件，不能当成「要搬动的目标」把自己删掉再搬。
    #[test]
    fn extension_case_change_keeps_sticker_alive() {
        let mut e = env("case-ext");
        let id = commands::create_sticker(
            &e.conn,
            &NewSticker { title: "笔记".into(), content: "正文".into(), ..Default::default() },
            &e.db_path,
        )
        .unwrap();
        e.sync();
        std::fs::rename(e.md("笔记.md"), e.md("笔记.MD")).unwrap();

        let report = e.sync();
        assert!(report.removed.is_empty(), "{report:?}");
        assert!(report.imported.is_empty(), "{report:?}");
        assert_eq!(sticker_repo::list_all(&e.conn).unwrap().len(), 1, "必须还是同一张便签");
        let s = sticker_repo::get(&e.conn, id).unwrap().unwrap();
        assert_eq!(s.content, "正文");
        assert!(e.md("笔记.MD").is_file(), "文件不能消失");
        let again = e.sync();
        assert!(again.removed.is_empty(), "{again:?}");
        assert_eq!(sticker_repo::list_all(&e.conn).unwrap().len(), 1);
    }

    /// 手动对账工具（默认 `#[ignore]`）：拿**真实**工作空间跑两轮同步并打印
    /// 分组/便签/磁盘树，确认 DB 与磁盘一致（第 2 轮应当是空的）。
    ///
    /// ```powershell
    /// $env:OII_WS_ROOT = 'C:\Users\ljz\Documents\我的文档'
    /// cargo test --lib real_workspace_check -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn real_workspace_check() {
        let root = match std::env::var("OII_WS_ROOT") {
            Ok(v) => v,
            Err(_) => panic!("请先设置 OII_WS_ROOT 指向真实工作空间根目录"),
        };
        let layout = Layout::at(Path::new(&root));
        let conn = open(&layout.db_path()).expect("打开 index.db 失败");
        crate::db::schema::run_migrations(&conn).expect("迁移失败");
        let db_path = layout.db_path().to_string_lossy().into_owned();
        let mut seen = HashSet::new();
        for round in 1..=2 {
            let report = sync_once(&conn, &db_path, &mut seen).unwrap();
            println!("[轮次 {round}] {report:?}");
        }
        dump(&conn, &layout);
    }

    /// 打印分组、便签与 `stickers/`、`assets/` 的磁盘树。
    fn dump(conn: &Connection, layout: &Layout) {
        println!("=== 分组 ===");
        for g in crate::db::group_repo::list(conn).unwrap() {
            println!("  id={} parent={:?} name={:?} color={:?}", g.id, g.parent_id, g.name, g.color);
        }
        println!("=== 便签 ===");
        for s in sticker_repo::list_all(conn).unwrap() {
            println!(
                "  id={} group={:?} title={:?} file_name={:?} 正文={} 字节",
                s.id,
                s.group_id,
                s.title,
                s.file_name,
                s.content.len()
            );
        }
        for dir in [layout.stickers_dir(), layout.assets_dir()] {
            println!("=== 磁盘 {} ===", dir.display());
            print_tree(&dir, "");
        }
    }

    fn print_tree(dir: &Path, indent: &str) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                println!("  {indent}[{}/]", entry.file_name().to_string_lossy());
                print_tree(&path, &format!("{indent}  "));
            } else {
                println!(
                    "  {indent}{} ({} 字节)",
                    entry.file_name().to_string_lossy(),
                    entry.metadata().map(|m| m.len()).unwrap_or(0)
                );
            }
        }
    }
}
