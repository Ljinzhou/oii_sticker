//! 业务命令层（纯函数封装 repo，供 `#[tauri::command]` 包装）。
//!
//! 本模块只做"模型 ←→ repo"的业务编排（默认值、级联、文本变换），
//! 不直接接触 Tauri 运行时；阶段 3 起在其上薄包一层 `#[tauri::command]`。

use anyhow::{bail, Result};
use rusqlite::Connection;

use crate::db::{config_repo, prefs_repo, sticker_repo, todo_block_repo, todo_repo};
use crate::db::sticker_repo::NewSticker;
use crate::editing;
use crate::models::{EffectivePrefs, Sticker, StickerPrefs, SystemConfig, TodoBlock, TodoBlockWithSticker, TodoPatch, TodoQueryFilter};
use crate::workspace::{layout, md_store, naming};

// ── 便签 CRUD ──

/// 当前工作空间根目录（从当前 DB 路径推算）。
pub fn ws_root(conn_path: &str) -> std::path::PathBuf {
    layout::root_from_db_path(std::path::Path::new(conn_path)).unwrap_or_default()
}

// ── 路径规则：分组即目录，便签即文件 ──
//
// 便签正文：`stickers/<分组路径>/<标题>.md`
// 附件：    `assets/<分组路径>/<标题>_<便签 id>/<文件>`
// 未分组（group_id = NULL）时没有分组那一层，直接落在根目录下。

/// 分组路径（相对 `stickers/`，正斜杠分隔）；未分组或分组已不存在 → 空串。
pub fn group_rel_path(conn: &Connection, group_id: Option<i64>) -> Result<String> {
    let Some(mut gid) = group_id else { return Ok(String::new()) };
    let mut parts: Vec<String> = Vec::new();
    let mut guard = 0usize;
    loop {
        guard += 1;
        if guard > 512 {
            bail!("分组层级过深（疑似存在环）");
        }
        let row: Option<(String, Option<i64>)> = conn
            .query_row(
                "SELECT name, parent_id FROM sticker_groups WHERE id = ?1",
                [gid],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok();
        match row {
            Some((name, parent)) => {
                // 写入侧已按系统规则校验；这里只做防御性清洗（旧数据可能带非法字符）
                parts.push(naming::sanitize_legacy(&name));
                match parent {
                    Some(p) => gid = p,
                    None => break,
                }
            }
            None => break,
        }
    }
    parts.reverse();
    Ok(parts.join("/"))
}

/// 分组在 `stickers/` 下的相对目录（资源管理器打开用）。
pub fn group_rel_dir(conn: &Connection, id: i64) -> Result<std::path::PathBuf> {
    Ok(std::path::PathBuf::from(group_rel_path(conn, Some(id))?))
}

/// 便签 md 的相对路径：`<分组路径>/<标题>.md`。
///
/// 标题非法（历史数据）时用清洗后的名字兜底：**校验在写入侧拒绝，读取侧绝不失败**——
/// 否则一条脏标题就会让整个工作空间的便签都打不开。
pub fn sticker_rel_path(conn: &Connection, s: &Sticker) -> Result<String> {
    sticker_rel_path_for(conn, s.group_id, &s.title)
}

/// 按显式标题算 md 相对路径（同步线程用：此刻 DB 里的标题/分组可能还没改完）。
pub fn sticker_rel_path_for(conn: &Connection, group_id: Option<i64>, title: &str) -> Result<String> {
    let file = layout::sticker_md_name(title)
        .unwrap_or_else(|_| format!("{}.md", naming::sanitize_legacy(title)));
    Ok(join_rel(&group_rel_path(conn, group_id)?, &file))
}

/// 附件目录的相对路径（相对 `assets/`）：`<分组路径>/<标题>_<便签 id>`。
pub fn sticker_asset_dir(conn: &Connection, s: &Sticker) -> Result<String> {
    Ok(join_rel(&group_rel_path(conn, s.group_id)?, &asset_leaf(s)))
}

/// 由分组目录 + 标题 + id 直接算附件目录相对路径（同步线程用：此刻 DB 可能还没改）。
pub fn asset_dir_rel(dir_rel: &str, title: &str, id: i64) -> String {
    join_rel(dir_rel, &format!("{}_{}", naming::sanitize_legacy(title), id))
}

/// 附件目录名：`<标题>_<便签 id>`（标题改名 / 分组搬迁时目录一并改名）。
fn asset_leaf(s: &Sticker) -> String {
    format!("{}_{}", naming::sanitize_legacy(&s.title), s.id)
}

fn join_rel(dir: &str, leaf: &str) -> String {
    if dir.is_empty() { leaf.to_string() } else { format!("{dir}/{leaf}") }
}

/// 自底向上删掉空目录；只要目录里还有东西就整棵保留（只记一条日志）。
///
/// 删分组/搬分组后用它替代 `remove_dir_all`：分组文件夹是用户能直接看到的普通文件夹，
/// 里面可能有用户自己放的非 md 文件、点开头的隐藏文件/目录（同步线程从不导入它们，
/// 也从没被计入"删了几张便签"），整棵删掉等于无声销毁用户数据且不进回收站。
fn prune_empty_dirs(dir: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return; // 目录不存在：本来就没东西要清
    };
    let children: Vec<std::path::PathBuf> =
        entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    for child in children {
        if child.is_dir() {
            prune_empty_dirs(&child);
        }
    }
    if let Err(e) = std::fs::remove_dir(dir) {
        tracing::warn!("[分组] 目录里还有内容，保留不删：{}（{e}）", dir.display());
    }
}

/// 组内重名检查：同一分组下不允许同名便签（md 文件名会撞车）。
pub fn ensure_title_free(
    conn: &Connection,
    group_id: Option<i64>,
    title: &str,
    exclude: Option<i64>,
) -> Result<()> {
    if let Some(other) = sticker_repo::find_by_title_in_group(conn, group_id, title, exclude)? {
        bail!("该分组下已有同名便签「{}」（id={}），请换一个名字", other.title, other.id);
    }
    Ok(())
}

/// 同级重名检查：同一个父级下不允许同名分组（文件夹会撞车）。
///
/// 除了名字本身，还要比**清洗后的目录名**：磁盘上的分组目录是
/// `naming::sanitize_legacy(name)`，旧数据里 `电子/嵌入式学习` 这种含非法字符的名字
/// 会被清洗成 `电子-嵌入式学习`。不查这一条，一个叫「电子/嵌入式学习」的旧分组和一个
/// 新建的「电子-嵌入式学习」会共用同一个文件夹，同标题便签就会写到同一个 md 上互相覆盖。
pub fn ensure_group_name_free(
    conn: &Connection,
    parent_id: Option<i64>,
    name: &str,
    exclude: Option<i64>,
) -> Result<()> {
    if let Some(other) =
        crate::db::group_repo::find_by_name_in_parent(conn, parent_id, name, exclude)?
    {
        bail!("同级下已有同名分组「{}」（id={}），请换一个名字", other.name, other.id);
    }
    let leaf = naming::sanitize_legacy(name);
    for other in crate::db::group_repo::list(conn)? {
        if Some(other.id) == exclude || other.parent_id != parent_id {
            continue;
        }
        if naming::sanitize_legacy(&other.name).eq_ignore_ascii_case(&leaf) {
            bail!("同级下已有同名分组「{}」（id={}，文件夹同名），请换一个名字", other.name, other.id);
        }
    }
    Ok(())
}

/// 便签正文里的附件链接随目录改名迁移：`assets/<old>/…` → `assets/<new>/…`。
///
/// 链接由后端按 `assets/<分组>/<标题>_<id>/<文件>` 生成，这里做同构替换；
/// 两种写法都照顾到：`assets/…` 与 `./assets/…`。
pub fn rewrite_asset_links(content: &str, old_dir: &str, new_dir: &str) -> String {
    if old_dir == new_dir || old_dir.is_empty() {
        return content.to_string();
    }
    content
        .replace(&format!("assets/{old_dir}/"), &format!("assets/{new_dir}/"))
        .replace(&format!("assets/{old_dir})"), &format!("assets/{new_dir})"))
        .replace(&format!("assets/{old_dir}\""), &format!("assets/{new_dir}\""))
}

/// 首次使用默认欢迎便签（仅在真实工作空间/首次迁移成功后创建，避免 bootstrap 库孤儿）。
pub fn welcome_sticker_new() -> NewSticker {
    NewSticker {
        title: "欢迎使用 oii_sticker".into(),
        content: r#"# 欢迎使用 oii_sticker

这是一张 **Markdown 便签** —— 你看到的一切都可以编辑。
点击右上角 ✎ 进入编辑模式，`Ctrl+S` 保存；双击便签可从收起状态唤醒。

## 写作

- **加粗**、*斜体*、~~删除线~~、`行内代码`、[链接](https://www.bilibili.com/)
- 引用：数据永远以你写的文字为准
- 代码块带语法高亮：

```rust
fn main() {
    println!("Hello, oii_sticker!");
}
```

- 数学公式：质能方程 $E = mc^2$

$$
\sum_{i=1}^{n} i = \frac{n(n+1)}{2}
$$

## 任务清单

普通待办，勾选即完成并自动记录历史：

- [ ] 勾选我试试（完成会进入「已完成」列表）
- [x] 这是一个已完成示例

在正文输入 `/todo` 可以插入独立的 **Todo 块**——它有自己的任务窗口，
支持子任务、提醒时间、截止时间与重复规则（设置里可调整默认值）。

## 数据保存在哪？

你的每张便签都是工作控件目录下的一个 `.md` 文件——可以用任何
Markdown 编辑器直接打开；任务与偏好则存放在同目录的 SQLite 数据库里。

打开 **系统设置 → 工作空间** 可以创建多个工作控件，
并在它们之间切换、备份（zip）或整体转移，把生活 / 工作 / 学习分开管理。

## 小技巧

- 点击 ⚙ 调整颜色、透明度与字号；置顶开关也在里面
- 编辑器里输入 `/` 呼出命令菜单：标题、列表、表格、代码块……
- 隐藏的便签不会占用内存，从托盘或主控台随时唤回"#.into(),
        pos_x: 200,
        pos_y: 140,
        width: 400,
        height: 520,
        opacity: 0.9,
        bg_color: Some("#FFF4D6".into()),
        ..Default::default()
    }
}

/// 创建默认欢迎便签（md 主存储随 create_sticker 一并落盘）。
pub fn create_welcome_sticker(conn: &Connection, db_path: &str) -> Result<i64> {
    create_sticker(conn, &welcome_sticker_new(), db_path)
}

/// 新建便签，返回自增 id；md 落盘（主存储），DB content 列保留作回退。
///
/// 标题即文件名：先按系统命名规则校验、再拒绝组内重名，md 写到
/// `<分组路径>/<标题>.md`，并把相对路径写回 `stickers.file_name`。
pub fn create_sticker(conn: &Connection, new: &sticker_repo::NewSticker, db_path: &str) -> Result<i64> {
    let title = naming::validate(naming::STICKER_LABEL, &new.title)?;
    ensure_title_free(conn, new.group_id, &title, None)?;
    let root = ws_root(db_path);
    let rel = join_rel(&group_rel_path(conn, new.group_id)?, &layout::sticker_md_name(&title)?);
    if md_store::sticker_path(&root, &rel)?.exists() {
        bail!("「{rel}」已经存在，请换一个名字");
    }
    let id = sticker_repo::insert(conn, new)?;
    sticker_repo::update(
        conn,
        id,
        &sticker_repo::StickerPatch {
            title: Some(title),
            file_name: Some(rel.clone()),
            ..Default::default()
        },
    )?;
    md_store::write(&root, &rel, &new.content)?;
    Ok(id)
}

/// 按 id 读取便签；md 内容优先（md 缺失回退 DB content 列）。
pub fn get_sticker(conn: &Connection, id: i64, db_path: &str) -> Result<Option<Sticker>> {
    let Some(mut sticker) = sticker_repo::get(conn, id)? else {
        return Ok(None);
    };
    let root = ws_root(db_path);
    let rel = sticker_rel_path(conn, &sticker)?;
    if let Some(content) = md_store::load(&root, &rel)? {
        sticker.content = content;
    }
    Ok(Some(sticker))
}

/// 列出全部便签。
pub fn list_stickers(conn: &Connection) -> Result<Vec<Sticker>> {
    sticker_repo::list_all(conn)
}

/// 部分更新便签；标题变化时 md 与附件目录一起改名，正文里的附件链接同步改写。
///
/// 返回值：因 content 变更而被清理的"无 `<todo-block>` 标签"根任务 id 列表。
/// （用户在编辑器中显式删除某个标签并保存，对应根任务 + 子任务应一并清掉，
/// 否则 Todo 窗口会继续显示这些"标签已无"的任务，造成"删除后复活"的错觉。）
pub fn update_sticker(
    conn: &Connection,
    id: i64,
    patch: &sticker_repo::StickerPatch,
    db_path: &str,
) -> Result<Vec<String>> {
    let root = ws_root(db_path);
    let mut patch = patch.clone();
    if let Some(current) = sticker_repo::get(conn, id)? {
        let group_path = group_rel_path(conn, current.group_id)?;
        let old_rel = sticker_rel_path(conn, &current)?;
        let old_asset = sticker_asset_dir(conn, &current)?;
        // 标题：系统命名规则 + 组内重名（文件名不能撞车）
        if let Some(raw) = patch.title.clone() {
            let title = naming::validate(naming::STICKER_LABEL, &raw)?;
            ensure_title_free(conn, current.group_id, &title, Some(id))?;
            patch.title = Some(title);
        }
        // 生效标题 → 新的 md 路径与附件目录名
        let title = patch.title.clone().unwrap_or_else(|| current.title.clone());
        let new_rel = join_rel(
            &group_path,
            &layout::sticker_md_name(&title)
                .unwrap_or_else(|_| format!("{}.md", naming::sanitize_legacy(&title))),
        );
        let mut probe = current.clone();
        probe.title = title.clone();
        let new_asset = join_rel(&group_path, &asset_leaf(&probe));

        // ① md 改名（标题变化）
        if new_rel != old_rel {
            md_store::move_path(
                &md_store::sticker_path(&root, &old_rel)?,
                &md_store::sticker_path(&root, &new_rel)?,
            )?;
        }
        // ② 附件目录改名 + 正文链接改写（目录名含标题）
        let mut content = patch.content.clone();
        if new_asset != old_asset {
            md_store::move_path_replacing(
                &md_store::asset_path(&root, &old_asset)?,
                &md_store::asset_path(&root, &new_asset)?,
            )?;
            let base = content.clone().unwrap_or_else(|| current.content.clone());
            let rewritten = rewrite_asset_links(&base, &old_asset, &new_asset);
            if rewritten != base {
                content = Some(rewritten);
            }
        }
        // ③ content（含链接改写结果）原子写 md（主存储），文件名按「生效标题」派生
        if let Some(text) = &content {
            md_store::write(&root, &new_rel, text)?;
        }
        patch.content = content;
        patch.file_name = Some(new_rel);
    }
    sticker_repo::update(conn, id, &patch)?;
    // 内容变更 → 清理"标签缺失"的孤儿根任务（用户已显式删标签，DB 不应保留）
    if let Some(content) = &patch.content {
        let tag_ids = todo_block_repo::tagged_ids(content);
        let deleted = todo_block_repo::delete_roots_not_in(conn, id, &tag_ids)?;
        Ok(deleted)
    } else {
        Ok(Vec::new())
    }
}

/// 单独暴露：扫描便签内容中无标签的根任务并删除（与 `update_sticker` 内部逻辑一致）。
/// 给需要"重新走一次清理"的场景（如重置 / 手动修复）使用。
pub fn collect_orphan_blocks(conn: &Connection, sticker_id: i64, content: &str) -> Result<Vec<String>> {
    let tag_ids = todo_block_repo::tagged_ids(content);
    todo_block_repo::delete_roots_not_in(conn, sticker_id, &tag_ids)
}

/// 删除便签（依赖外键级联清理），md 与附件目录一并移除。
pub fn delete_sticker(conn: &Connection, id: i64, db_path: &str) -> Result<()> {
    if let Some(sticker) = sticker_repo::get(conn, id)? {
        let root = ws_root(db_path);
        let _ = md_store::remove(&root, &sticker_rel_path(conn, &sticker).unwrap_or_default());
        if let Ok(dir) = sticker_asset_dir(conn, &sticker) {
            let _ = md_store::remove_any(&md_store::asset_path(&root, &dir)?);
        }
        // 旧布局（assets/<便签 id>/）兜底清理：迁移前落盘的附件
        let legacy = root.join("assets").join(id.to_string());
        if legacy.is_dir() {
            let _ = std::fs::remove_dir_all(legacy);
        }
    }
    sticker_repo::delete(conn, id)
}

// ── 便签分组（分组即文件夹）──

pub fn list_groups(conn: &Connection) -> Result<Vec<crate::models::StickerGroup>> {
    crate::db::group_repo::list(conn)
}

/// 新建分组；`parent_id` 为 None 表示顶层（可无限嵌套）。
///
/// 名称按系统规则校验、同级重名拒绝，并**立刻在 `stickers/` 下建出同名文件夹**：
/// 分组一建，资源管理器里就该看到它。
pub fn create_group(
    conn: &Connection,
    db_path: &str,
    name: &str,
    parent_id: Option<i64>,
) -> Result<crate::models::StickerGroup> {
    let name = naming::validate(naming::GROUP_LABEL, name)?;
    ensure_group_name_free(conn, parent_id, &name, None)?;
    let group = crate::db::group_repo::create(conn, &name, parent_id)?;
    let root = ws_root(db_path);
    let dir = md_store::sticker_path(&root, &group_rel_path(conn, Some(group.id))?)?;
    std::fs::create_dir_all(&dir).map_err(|e| anyhow::anyhow!("创建分组文件夹失败：{e}"))?;
    Ok(group)
}

/// 分组改名：文件夹（含子分组、便签 md、附件目录）整体改名，正文链接同步改写。
pub fn rename_group(conn: &Connection, db_path: &str, id: i64, name: &str) -> Result<()> {
    let name = naming::validate(naming::GROUP_LABEL, name)?;
    let current = crate::db::group_repo::get(conn, id)?
        .ok_or_else(|| anyhow::anyhow!("分组不存在"))?;
    ensure_group_name_free(conn, current.parent_id, &name, Some(id))?;
    let root = ws_root(db_path);
    let old_dir = group_rel_path(conn, Some(id))?;
    crate::db::group_repo::rename(conn, id, &name)?;
    let new_dir = group_rel_path(conn, Some(id))?;
    relocate_group(conn, &root, &old_dir, &new_dir, id)
}

/// 设置分组颜色（None = 清除颜色）。
pub fn set_group_color(conn: &Connection, id: i64, color: Option<&str>) -> Result<()> {
    crate::db::group_repo::set_color(conn, id, color)
}

/// 同级重排（拖动调整顺序）；`ids` 为该父级下的目标顺序，其它层级自动忽略。
pub fn reorder_groups(conn: &Connection, parent_id: Option<i64>, ids: &[i64]) -> Result<()> {
    crate::db::group_repo::reorder(conn, parent_id, ids)
}

/// 移动分组到新父级（None = 顶层）；拒绝放入自身或自己的子树；文件夹随之搬家。
pub fn move_group(conn: &Connection, db_path: &str, id: i64, parent_id: Option<i64>) -> Result<()> {
    let old_dir = group_rel_path(conn, Some(id))?;
    crate::db::group_repo::move_to_parent(conn, id, parent_id)?;
    let root = ws_root(db_path);
    let new_dir = group_rel_path(conn, Some(id))?;
    relocate_group(conn, &root, &old_dir, &new_dir, id)
}

/// 分组子树（含自身）内的全部便签 id（删组前的父便签保护用）。
pub fn sticker_ids_in_group(conn: &Connection, id: i64) -> Result<Vec<i64>> {
    crate::db::group_repo::sticker_ids_in_subtree(conn, id)
}

/// 分组搬迁：把该分组（含子分组）的目录整体改名/移动，并同步便签的 `file_name` 与正文链接。
///
/// `old_rel` / `new_rel` 是相对 `stickers/` 的目录路径（顶层分组 → `随手记`）。
/// 目录整体 `rename`（快且原子），而不是逐个文件搬；随后只重写受影响的**文本引用**：
/// 附件链接里含分组路径，所以该子树里每条便签的正文都要过一遍
/// `assets/<旧分组路径>/` → `assets/<新分组路径>/`。
fn relocate_group(
    conn: &Connection,
    root: &std::path::Path,
    old_rel: &str,
    new_rel: &str,
    group_id: i64,
) -> Result<()> {
    if old_rel == new_rel {
        return Ok(());
    }
    let stickers = layout::Layout::at(root).stickers_dir();
    let move_dir = |from: &std::path::Path, to: &std::path::Path| -> Result<()> {
        if from == to {
            return Ok(());
        }
        if !from.exists() {
            // 空分组（没有便签）时目录可能还不存在：直接建出目标目录
            std::fs::create_dir_all(to)?;
            return Ok(());
        }
        if to.is_file() {
            bail!("目标位置已有同名文件，请先处理：{}", to.display());
        }
        if to.is_dir() && std::fs::read_dir(to).map(|mut it| it.next().is_some()).unwrap_or(false) {
            bail!("目标目录已存在且非空，请先处理：{}", to.display());
        }
        md_store::move_path(from, to)
    };
    move_dir(
        &md_store::rel_join(&stickers, old_rel)?,
        &md_store::rel_join(&stickers, new_rel)?,
    )?;
    move_dir(
        &md_store::asset_path(root, old_rel)?,
        &md_store::asset_path(root, new_rel)?,
    )?;

    for sid in crate::db::group_repo::sticker_ids_in_subtree(conn, group_id)? {
        let Some(s) = sticker_repo::get(conn, sid)? else { continue };
        let rel = sticker_rel_path(conn, &s)?;
        let new_asset = sticker_asset_dir(conn, &s)?;
        let old_asset = join_rel(old_rel, &asset_leaf(&s));
        let content = md_store::load(root, &rel)?.unwrap_or_else(|| s.content.clone());
        let rewritten = rewrite_asset_links(&content, &old_asset, &new_asset);
        let stale_path = s.file_name.as_deref() != Some(rel.as_str());
        if rewritten != content || stale_path {
            md_store::write(root, &rel, &rewritten)?;
            sticker_repo::update(
                conn,
                sid,
                &sticker_repo::StickerPatch {
                    content: Some(rewritten),
                    file_name: Some(rel),
                    ..Default::default()
                },
            )?;
        }
    }
    Ok(())
}

/// 删除分组。mode="with-stickers" 时连带删除组内便签（含 md 与附件文件清理），
/// 返回被删除的便签数量；mode="to-default" 时便签回到未分组，文件搬回 `stickers/` 根，返回 0。
pub fn delete_group(conn: &Connection, id: i64, mode: &str, db_path: &str) -> Result<usize> {
    let root = ws_root(db_path);
    let stickers_dir = layout::Layout::at(&root).stickers_dir();
    let rel_dir = group_rel_path(conn, Some(id))?;
    match mode {
        "with-stickers" => {
            // md / 附件路径必须在删行之前算好（行没了就推不出标题与分组）
            let mut victims = Vec::new();
            for sid in crate::db::group_repo::sticker_ids_in_subtree(conn, id)? {
                if let Some(s) = sticker_repo::get(conn, sid)? {
                    victims.push((sticker_rel_path(conn, &s)?, sticker_asset_dir(conn, &s)?));
                }
            }
            let removed = crate::db::group_repo::delete(conn, id, mode)?;
            for (rel, asset) in victims {
                let _ = md_store::remove(&root, &rel);
                if let Ok(dir) = md_store::asset_path(&root, &asset) {
                    let _ = md_store::remove_any(&dir);
                }
            }
            // 分组目录（含子分组）里我们自己写进去的东西已经删完了；
            // 这里只**自底向上删空目录**，绝不 `remove_dir_all`：
            // 用户往分组文件夹里放的非 md 文件（图片、PDF、工程文件）和点开头的隐藏文件/目录
            // 本来就不参与同步、也从没被计数，删掉它们等于无声销毁用户数据，且不进回收站。
            // 目录非空时保留，只记一条日志。
            if let Ok(dir) = md_store::rel_join(&stickers_dir, &rel_dir) {
                prune_empty_dirs(&dir);
            }
            if let Ok(dir) = md_store::asset_path(&root, &rel_dir) {
                prune_empty_dirs(&dir);
            }
            Ok(removed.len())
        }
        "to-default" => {
            // 旧路径必须在删行之前算好：to-default 会把 group_id 置空，
            // 删完再推就只剩新路径，搬文件会变成"源＝目标"的空操作。
            let mut victims = Vec::new();
            for sid in crate::db::group_repo::sticker_ids_in_subtree(conn, id)? {
                if let Some(s) = sticker_repo::get(conn, sid)? {
                    let old_rel = sticker_rel_path(conn, &s)?;
                    let old_asset = sticker_asset_dir(conn, &s)?;
                    victims.push((s, old_rel, old_asset));
                }
            }
            // 搬回 `stickers/` 根目录之前先查一遍根目录里的同名冲突：
            // 未分组的「日志」与某分组里的「日志」可以并存（重名只在本组内禁止），
            // 但如果先把文件搬过去、再去对账，两边会互相顶掉，最后连便签带附件一起消失。
            // 所以宁可整单拒绝，让用户先改名（跨组撞名极少见）。
            for (s, old_rel, _) in &victims {
                ensure_title_free(conn, None, &s.title, Some(s.id))?;
                // 落盘那一侧也要在动手之前拦下来：根目录里可能已经有同名文件（用户刚拖进来、
                // 同步还没来得及认领的 md，或库里根本没有对应便签的孤儿文件）。
                // 中途失败会留下一半搬完的便签，所以这里先算好目标路径并检查。
                let rel = layout::sticker_md_name(&s.title)
                    .unwrap_or_else(|_| format!("{}.md", naming::sanitize_legacy(&s.title)));
                let source = md_store::sticker_path(&root, old_rel)?;
                let target = md_store::sticker_path(&root, &rel)?;
                if target.exists() && !md_store::same_path(&source, &target) {
                    bail!("便签根目录里已有文件「{rel}」，请先给它改名或移走");
                }
            }
            crate::db::group_repo::delete(conn, id, mode)?;
            for (s, old_rel, old_asset) in victims {
                let rel = layout::sticker_md_name(&s.title)
                    .unwrap_or_else(|_| format!("{}.md", naming::sanitize_legacy(&s.title)));
                let new_asset = asset_leaf(&s);
                md_store::move_path(
                    &md_store::sticker_path(&root, &old_rel)?,
                    &md_store::sticker_path(&root, &rel)?,
                )?;
                md_store::move_path_replacing(
                    &md_store::asset_path(&root, &old_asset)?,
                    &md_store::asset_path(&root, &new_asset)?,
                )?;
                let content = md_store::load(&root, &rel)?.unwrap_or_else(|| s.content.clone());
                let rewritten = rewrite_asset_links(&content, &old_asset, &new_asset);
                md_store::write(&root, &rel, &rewritten)?;
                sticker_repo::update(
                    conn,
                    s.id,
                    &sticker_repo::StickerPatch {
                        content: Some(rewritten),
                        file_name: Some(rel.clone()),
                        ..Default::default()
                    },
                )?;
            }
            // 空掉的分组目录一并清掉（附件目录同理）
            if let Ok(dir) = md_store::rel_join(&stickers_dir, &rel_dir) {
                let _ = std::fs::remove_dir(dir);
            }
            if let Ok(dir) = md_store::asset_path(&root, &rel_dir) {
                let _ = std::fs::remove_dir(dir);
            }
            Ok(0)
        }
        _ => bail!("未知的删除模式：{mode}"),
    }
}

/// 把便签移动到另一个分组：md 与附件目录一起搬家，正文链接同步改写。
/// （文件先搬、DB 后更新：万一 DB 失败，同步线程能按新位置自愈。）
pub fn move_sticker_group(
    conn: &Connection,
    sticker_id: i64,
    group_id: Option<i64>,
    db_path: &str,
) -> Result<()> {
    let current = sticker_repo::get(conn, sticker_id)?
        .ok_or_else(|| anyhow::anyhow!("便签不存在"))?;
    if let Some(gid) = group_id {
        crate::db::group_repo::get(conn, gid)?
            .ok_or_else(|| anyhow::anyhow!("目标分组不存在"))?;
    }
    ensure_title_free(conn, group_id, &current.title, Some(sticker_id))?;
    let root = ws_root(db_path);
    let mut probe = current.clone();
    probe.group_id = group_id;
    let new_dir = group_rel_path(conn, group_id)?;
    let old_rel = sticker_rel_path(conn, &current)?;
    let old_asset = sticker_asset_dir(conn, &current)?;
    let new_rel = join_rel(
        &new_dir,
        &layout::sticker_md_name(&current.title)
            .unwrap_or_else(|_| format!("{}.md", naming::sanitize_legacy(&current.title))),
    );
    let new_asset = join_rel(&new_dir, &asset_leaf(&probe));
    md_store::move_path(
        &md_store::sticker_path(&root, &old_rel)?,
        &md_store::sticker_path(&root, &new_rel)?,
    )?;
    md_store::move_path_replacing(
        &md_store::asset_path(&root, &old_asset)?,
        &md_store::asset_path(&root, &new_asset)?,
    )?;
    let content = md_store::load(&root, &new_rel)?.unwrap_or_else(|| current.content.clone());
    let rewritten = rewrite_asset_links(&content, &old_asset, &new_asset);
    md_store::write(&root, &new_rel, &rewritten)?;
    crate::db::group_repo::move_sticker(conn, sticker_id, group_id)?;
    sticker_repo::update(
        conn,
        sticker_id,
        &sticker_repo::StickerPatch {
            content: if rewritten != content { Some(rewritten) } else { None },
            file_name: Some(new_rel),
            ..Default::default()
        },
    )?;
    Ok(())
}

/// 组内便签重排（主控台拖拽调整位置）：`ids` 为该分组（含未分组）的完整新顺序。
pub fn reorder_stickers(conn: &Connection, ids: &[i64]) -> Result<()> {
    sticker_repo::reorder(conn, ids)
}

/// ── 粘贴图片（附件）──

/// 允许的图片扩展名（文件名后缀白名单）。
const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "bmp"];
/// 单张图片大小上限（base64 解码后的字节数）：32 MiB，防误传大文件把工作空间撑爆。
const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;

/// 便签附件落盘：写入
/// `<工作空间>/assets/<分组路径>/<便签标题>_<便签 id>/粘贴-YYYYMMDD-HHmmss.<ext>`，
/// 返回**相对工作空间**的 Markdown 可用路径（`assets/<分组>/<标题>_<id>/<文件>`，正斜杠）。
///
/// - 便签必须存在（避免往一个不存在的便签目录写文件）；
/// - 文件名由后端生成（只收扩展名），不信任调用方给的路径 —— 从根上杜绝目录穿越；
/// - 同一秒内多张图自动加 `-2`/`-3` 后缀，不覆盖已有文件；
/// - 便签改名 / 换分组时整个附件目录跟着搬（见 `update_sticker` / `move_sticker_group`）。
pub fn save_sticker_image(
    conn: &Connection,
    db_path: &str,
    sticker_id: i64,
    ext: &str,
    data: &[u8],
) -> Result<String> {
    let sticker = sticker_repo::get(conn, sticker_id)?
        .ok_or_else(|| anyhow::anyhow!("便签不存在"))?;
    if data.is_empty() {
        bail!("图片内容为空");
    }
    if data.len() > MAX_IMAGE_BYTES {
        bail!("图片过大（超过 {} MB）", MAX_IMAGE_BYTES / 1024 / 1024);
    }
    let ext = ext.trim().trim_start_matches('.').to_ascii_lowercase();
    if !IMAGE_EXTS.contains(&ext.as_str()) {
        bail!("不支持的图片格式：{ext}");
    }

    let root = ws_root(db_path);
    let dir_rel = sticker_asset_dir(conn, &sticker)?;
    let dir = md_store::asset_path(&root, &dir_rel)?;
    std::fs::create_dir_all(&dir).map_err(|e| anyhow::anyhow!("创建图片目录失败：{e}"))?;

    let stamp = local_timestamp();
    let mut name = format!("粘贴-{stamp}.{ext}");
    let mut seq = 2;
    while dir.join(&name).exists() {
        name = format!("粘贴-{stamp}-{seq}.{ext}");
        seq += 1;
    }
    let target = dir.join(&name);
    naming::ensure_path_fits(&target)?;
    std::fs::write(&target, data).map_err(|e| anyhow::anyhow!("写入图片失败：{e}"))?;
    tracing::info!(
        "[图片] 粘贴落盘 sticker={sticker_id} → {}",
        target.display()
    );
    Ok(format!("assets/{dir_rel}/{name}"))
}

/// 本地时间戳 "YYYYMMDD-HHmmss"（取不到本地时区时退化为 UTC）。
fn local_timestamp() -> String {
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}

/// base64 → 字节（粘贴图片经 IPC 以 base64 字符串传输）。
pub fn decode_base64_image(data_base64: &str) -> Result<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(data_base64.trim())
        .map_err(|e| anyhow::anyhow!("图片数据解码失败：{e}"))
}

// ── 偏好 ──

/// 写入/覆盖便签偏好。
pub fn update_sticker_prefs(conn: &Connection, prefs: &StickerPrefs) -> Result<()> {
    prefs_repo::upsert(conn, prefs)
}

/// 重置便签偏好（删除该行，恢复系统默认）。
pub fn reset_sticker_prefs(conn: &Connection, sticker_id: i64) -> Result<()> {
    prefs_repo::delete(conn, sticker_id)
}

/// 计算便签最终生效偏好（prefs → sticker.bg_color → system → 兜底）。
pub fn effective_prefs(
    conn: &Connection,
    config: &SystemConfig,
    sticker_id: i64,
) -> Result<EffectivePrefs> {
    let prefs = prefs_repo::get(conn, sticker_id)?.unwrap_or_default();
    let sticker_bg = sticker_repo::get(conn, sticker_id)?.and_then(|s| s.bg_color);
    Ok(config.effective(&prefs, sticker_bg.as_deref()))
}

// ── 配置 ──

/// 读取全部配置（返回 SystemConfig 快照）。
pub fn get_config(conn: &Connection) -> Result<SystemConfig> {
    config_repo::load_all(conn)
}

/// 写入单条配置。
pub fn set_config(conn: &Connection, key: &str, value: &str) -> Result<()> {
    config_repo::set(conn, key, value)
}

// ── 待办 ──

/// 翻转 markdown 内容第 `line` 行的 todo 状态并返回新内容。
pub fn toggle_todo(content: &str, line: usize) -> Option<String> {
    editing::toggle_todo_in_content(content, line)
}

/// 翻转便签内 todo 并落库（读取 md 主存储 → 文本变换 → 原子写 md + 双写 DB）。
pub fn toggle_todo_in_sticker(conn: &Connection, sticker_id: i64, line: usize, db_path: &str) -> Result<bool> {
    let Some(sticker) = get_sticker(conn, sticker_id, db_path)? else {
        return Ok(false);
    };
    let Some(new_content) = editing::toggle_todo_in_content(&sticker.content, line) else {
        return Ok(false);
    };
    update_sticker(
        conn,
        sticker_id,
        &sticker_repo::StickerPatch {
            content: Some(new_content),
            ..Default::default()
        },
        db_path,
    )?;
    Ok(true)
}

/// 列出某个便签的所有 todo。
pub fn list_todos(conn: &Connection, sticker_id: i64) -> Result<Vec<crate::models::TodoItem>> {
    todo_repo::list_by_sticker(conn, sticker_id)
}

// ── 独立 Todo 块 ──

pub fn list_todo_blocks(conn: &Connection, sticker_id: i64) -> Result<(Vec<TodoBlock>, bool)> {
    // 列表前先为无标记的根任务补写标记（幂等），保证块与正文一致；
    // 返回是否补写了标记（调用方据此通知便签窗口刷新正文）。
    let retagged = !todo_block_repo::retag_orphans_for_sticker(conn, sticker_id)?.is_empty();
    Ok((todo_block_repo::list_by_sticker(conn, sticker_id)?, retagged))
}

/// 跨便签 Todo 聚合查询（主控台「任务总览」页与 AI 工具层共用）。
/// 只读聚合，不触发孤儿补写（无正文关联的写入副作用）。
pub fn list_all_todos(conn: &Connection, filter: &TodoQueryFilter) -> Result<Vec<TodoBlockWithSticker>> {
    todo_block_repo::list_all_todos(conn, filter)
}

pub fn get_todo_block(conn: &Connection, id: &str) -> Result<Option<TodoBlock>> {
    todo_block_repo::get(conn, id)
}

pub fn create_todo_block(conn: &Connection, sticker_id: i64, parent_id: Option<&str>) -> Result<TodoBlock> {
    todo_block_repo::create(conn, sticker_id, parent_id)
}

pub fn update_todo_block(conn: &Connection, id: &str, patch: &TodoPatch) -> Result<TodoBlock> {
    todo_block_repo::update(conn, id, patch)
}

pub fn delete_todo_block(conn: &Connection, id: &str) -> Result<Option<i64>> {
    todo_block_repo::delete(conn, id)
}

/// 确认收到提醒（红点点击）：清触发标记 + 记确认时刻，返回更新后的块。
pub fn ack_todo_alerts(conn: &Connection, id: &str) -> Result<Option<TodoBlock>> {
    todo_block_repo::ack_alerts(conn, id)
}

/// 把 `<todo-block>` 标记追加到便签内容末尾（幂等；已含标记则跳过）。
/// 供 Todo 窗口新建根任务后同步标记，保证块与正文一致、不产生孤儿。
pub fn sync_todo_marker(conn: &Connection, sticker_id: i64, id: &str, db_path: &str) -> Result<bool> {
    let Some(sticker) = get_sticker(conn, sticker_id, db_path)? else {
        bail!("便签不存在");
    };
    let tagged = todo_block_repo::tagged_ids(&sticker.content);
    if tagged.iter().any(|t| t == id) {
        return Ok(false);
    }
    let tag = format!("<todo-block id=\"{id}\"></todo-block>");
    let next = if sticker.content.trim().is_empty() {
        tag
    } else {
        format!("{}\n\n{tag}", sticker.content.trim_end())
    };
    update_sticker(conn, sticker_id, &sticker_repo::StickerPatch {
        content: Some(next),
        ..Default::default()
    }, db_path)?;
    Ok(true)
}

/// 全局补写孤儿 Todo 块标记（启动时调用一次），返回每便签被补写的 id 列表供广播。
pub fn retag_orphan_todos(conn: &Connection) -> Result<Vec<(i64, Vec<String>)>> {
    todo_block_repo::retag_all_orphans(conn)
}

/// 重排任务（拖拽排序），返回所属便签 id（空列表返回 None）。
pub fn reorder_todo(conn: &Connection, ids: &[String]) -> Result<Option<i64>> {
    if ids.is_empty() {
        return Ok(None);
    }
    let sticker_id = todo_block_repo::get(conn, &ids[0])?.map(|b| b.sticker_id);
    todo_block_repo::reorder(conn, ids)?;
    Ok(sticker_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema;
    use std::path::PathBuf;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        schema::run_migrations(&conn).unwrap();
        conn
    }

    /// 临时工作空间根目录：ensure_layout 后返回 (root, db_path)。
    fn tmp_ws(tag: &str) -> (PathBuf, String) {
        let d = std::env::temp_dir().join(format!("ws-cmd-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        crate::workspace::layout::ensure_layout(&crate::workspace::layout::Layout::at(&d), "t")
            .unwrap();
        let db = d.join("data").join("index.db").to_string_lossy().into_owned();
        (d, db)
    }

    /// 粘贴图片：落盘到 assets/<分组路径>/<标题>_<id>/、同名自动加序号、扩展名/空数据/便签不存在都拒绝。
    #[test]
    fn save_sticker_image_writes_into_assets_and_validates_input() {
        use base64::Engine;
        let conn = test_conn();
        let (root, db_path) = tmp_ws("image");
        let id = create_sticker(
            &conn,
            &sticker_repo::NewSticker { title: "图片便签".into(), ..Default::default() },
            &db_path,
        )
        .unwrap();
        let png: Vec<u8> = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 1, 2, 3];

        let rel = save_sticker_image(&conn, &db_path, id, "png", &png).unwrap();
        let dir = format!("assets/图片便签_{id}/");
        assert!(rel.starts_with(&dir), "got: {rel}");
        assert!(rel.ends_with(".png"), "got: {rel}");
        let abs = root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        assert_eq!(std::fs::read(&abs).unwrap(), png, "图片字节应原样落盘");

        // 同一秒内的第二张：不覆盖，追加 -2
        let rel2 = save_sticker_image(&conn, &db_path, id, ".PNG", &png).unwrap();
        assert!(rel2.starts_with(&dir), "got: {rel2}");
        assert!(rel2.contains("-2.png"), "got: {rel2}");
        assert_ne!(rel, rel2);
        assert_eq!(std::fs::read(&root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR))).unwrap(), png);

        // 非法输入
        assert!(save_sticker_image(&conn, &db_path, id, "exe", &png).is_err(), "扩展名白名单");
        assert!(save_sticker_image(&conn, &db_path, id, "png", &[]).is_err(), "空数据");
        assert!(save_sticker_image(&conn, &db_path, 9999, "png", &png).is_err(), "便签必须存在");

        // base64 往返（IPC 传输路径）
        let encoded = base64::engine::general_purpose::STANDARD.encode(&png);
        assert_eq!(decode_base64_image(&encoded).unwrap(), png);
        assert!(decode_base64_image("not base64!!").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sticker_lifecycle() {
        let conn = test_conn();
        let (root, db_path) = tmp_ws("life");
        let id = create_sticker(
            &conn,
            &sticker_repo::NewSticker {
                title: "业务层便签".into(),
                content: "- [ ] 任务一\n- [ ] 任务二".into(),
                ..Default::default()
            },
            &db_path,
        )
        .unwrap();

        // todo 翻转并落库
        assert!(toggle_todo_in_sticker(&conn, id, 0, &db_path).unwrap());
        let s = get_sticker(&conn, id, &db_path).unwrap().unwrap();
        assert!(s.content.starts_with("- [x] 任务一"), "got: {}", s.content);
        assert!(!toggle_todo_in_sticker(&conn, id, 99, &db_path).unwrap());

        // 偏好与 effective
        update_sticker_prefs(
            &conn,
            &StickerPrefs {
                sticker_id: id,
                opacity: Some(0.66),
                ..Default::default()
            },
        )
        .unwrap();
        let cfg = get_config(&conn).unwrap();
        let eff = effective_prefs(&conn, &cfg, id).unwrap();
        assert_eq!(eff.opacity, 0.66);

        delete_sticker(&conn, id, &db_path).unwrap();
        assert!(get_sticker(&conn, id, &db_path).unwrap().is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sticker_md_is_primary_storage() {
        let conn = test_conn();
        let (root, db_path) = tmp_ws("md");
        let id = create_sticker(
            &conn,
            &sticker_repo::NewSticker {
                title: "主存储".into(),
                content: "# 正文A".into(),
                ..Default::default()
            },
            &db_path,
        )
        .unwrap();
        let rel = sticker_rel_path(&conn, &get_sticker(&conn, id, &db_path).unwrap().unwrap()).unwrap();
        assert_eq!(rel, "主存储.md", "未分组便签的 md 直接放在 stickers/ 下");
        let file = root.join("stickers").join(&rel);
        assert!(file.exists(), "create 后 md 文件缺失");
        assert_eq!(md_store::load(&root, &rel).unwrap().unwrap(), "# 正文A");

        // content 更新 → md 同步写入（主存储）
        let new_content = "# 正文B".to_string();
        update_sticker(
            &conn,
            id,
            &sticker_repo::StickerPatch { content: Some(new_content.clone()), ..Default::default() },
            &db_path,
        )
        .unwrap();
        assert_eq!(md_store::load(&root, &rel).unwrap().unwrap(), "# 正文B");

        // 标题变更 → md 文件重命名（内容跟着搬）
        update_sticker(
            &conn,
            id,
            &sticker_repo::StickerPatch { title: Some("新标题".into()), ..Default::default() },
            &db_path,
        )
        .unwrap();
        assert!(!file.exists(), "旧文件未移除");
        let new_rel = "新标题.md";
        let new_file = root.join("stickers").join(new_rel);
        assert!(new_file.exists(), "新文件未生成");
        assert_eq!(md_store::load(&root, new_rel).unwrap().unwrap(), "# 正文B");

        // file_name 列缓存了新路径（外部编辑器改文件后靠它认领）
        assert_eq!(
            sticker_repo::get(&conn, id).unwrap().unwrap().file_name.as_deref(),
            Some(new_rel)
        );

        // get 以 md 为准
        let s = get_sticker(&conn, id, &db_path).unwrap().unwrap();
        assert_eq!(s.title, "新标题");
        assert_eq!(s.content, "# 正文B");

        // 删除 → md 一并移除
        delete_sticker(&conn, id, &db_path).unwrap();
        assert!(!new_file.exists());
        assert!(get_sticker(&conn, id, &db_path).unwrap().is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn update_with_title_and_content_writes_to_new_file_and_db() {
        let conn = test_conn();
        let (root, db_path) = tmp_ws("both");
        let id = create_sticker(
            &conn,
            &sticker_repo::NewSticker {
                title: "旧题".into(),
                content: "X".into(),
                ..Default::default()
            },
            &db_path,
        )
        .unwrap();

        // 组合补丁：标题+内容同时变更 → md 重命名到新标题文件且写入新内容
        update_sticker(
            &conn,
            id,
            &sticker_repo::StickerPatch {
                title: Some("新题".into()),
                content: Some("Y".into()),
                ..Default::default()
            },
            &db_path,
        )
        .unwrap();

        let old_rel = "旧题.md";
        let new_rel = "新题.md";
        assert!(!root.join("stickers").join(old_rel).exists(), "旧文件不应残留");
        assert_eq!(
            md_store::load(&root, new_rel).unwrap().unwrap(),
            "Y",
            "新文件名应为 Y"
        );

        // get 以 md 为准
        let s = get_sticker(&conn, id, &db_path).unwrap().unwrap();
        assert_eq!(s.title, "新题");
        assert_eq!(s.content, "Y");

        // DB content 列双写验证（回退保险）
        let row = sticker_repo::get(&conn, id).unwrap().unwrap();
        assert_eq!(row.content, "Y");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn get_sticker_falls_back_to_db_content_when_md_missing() {
        let conn = test_conn();
        let (root, db_path) = tmp_ws("fallback");
        let id = create_sticker(
            &conn,
            &sticker_repo::NewSticker {
                title: "回退".into(),
                content: "DB 内容".into(),
                ..Default::default()
            },
            &db_path,
        )
        .unwrap();

        let file = "回退.md";
        let path = root.join("stickers").join(file);
        assert!(path.exists());
        std::fs::remove_file(&path).unwrap();

        let s = get_sticker(&conn, id, &db_path).unwrap().unwrap();
        assert_eq!(s.content, "DB 内容", "md 缺失应回退 DB content 列");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn welcome_sticker_created_with_md_and_duplicate_rejected() {
        let conn = test_conn();
        let (root, db_path) = tmp_ws("welcome");
        let id = create_welcome_sticker(&conn, &db_path).unwrap();
        let s = get_sticker(&conn, id, &db_path).unwrap().unwrap();
        assert_eq!(s.title, "欢迎使用 oii_sticker");
        assert!(s.content.contains("欢迎使用"));
        let rel = sticker_rel_path(&conn, &s).unwrap();
        assert!(root.join("stickers").join(&rel).exists(), "欢迎便签 md 应落盘");
        // 标题即文件名：同分组重名＝同一路径，必须被拒绝（调用方靠"库为空"判断，不会重复创建）
        let err = create_welcome_sticker(&conn, &db_path).unwrap_err().to_string();
        assert!(err.contains("同名"), "重名应被拒绝：{err}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// with-stickers 删组：DB 行被 group_repo 事务删除前先收集 md 与附件目录，
    /// 删完把整棵分组目录一并移除；to-default 则把便签 md 与附件搬回便签根。
    #[test]
    fn delete_group_with_stickers_cleans_md_and_assets() {
        let conn = test_conn();
        let (root, db_path) = tmp_ws("group-del");
        let g = crate::db::group_repo::create(&conn, "待删组", None).unwrap();
        let id = create_sticker(
            &conn,
            &sticker_repo::NewSticker {
                title: "组内便签".into(),
                content: "# X".into(),
                ..Default::default()
            },
            &db_path,
        )
        .unwrap();
        move_sticker_group(&conn, id, Some(g.id), &db_path).unwrap();

        let file = root.join("stickers").join("待删组").join("组内便签.md");
        assert!(file.exists(), "前置：组内便签 md 已随分组落盘");
        let asset_dir = root
            .join("assets")
            .join("待删组")
            .join(format!("组内便签_{id}"));
        std::fs::create_dir_all(&asset_dir).unwrap();
        std::fs::write(asset_dir.join("x.png"), b"png").unwrap();

        let n = delete_group(&conn, g.id, "with-stickers", &db_path).unwrap();
        assert_eq!(n, 1);
        assert!(!file.exists(), "组内便签 md 应被清除");
        assert!(!asset_dir.exists(), "附件目录应一并移除");
        assert!(!root.join("stickers").join("待删组").exists(), "整棵分组目录应被移除");
        assert!(get_sticker(&conn, id, &db_path).unwrap().is_none());
        assert!(crate::db::group_repo::get(&conn, g.id).unwrap().is_none());

        // to-default：返回 0，便签保留并把 md / 附件搬回便签根目录
        let g2 = crate::db::group_repo::create(&conn, "保留组", None).unwrap();
        let s2 = create_sticker(
            &conn,
            &sticker_repo::NewSticker { title: "留用".into(), content: "Y".into(), ..Default::default() },
            &db_path,
        )
        .unwrap();
        move_sticker_group(&conn, s2, Some(g2.id), &db_path).unwrap();
        assert!(root.join("stickers").join("保留组").join("留用.md").exists(), "前置：已在分组里");
        assert_eq!(delete_group(&conn, g2.id, "to-default", &db_path).unwrap(), 0);
        assert!(root.join("stickers").join("留用.md").exists(), "to-default 应把 md 搬回便签根");
        assert!(!root.join("stickers").join("保留组").join("留用.md").exists());
        assert!(get_sticker(&conn, s2, &db_path).unwrap().is_some());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// with-stickers 删分组时，用户在分组文件夹里自己放的东西（图片、PDF、点开头的隐藏目录）
    /// 既不被同步、也从没被计数，绝不能跟着一起删掉——分组文件夹就是普通文件夹。
    #[test]
    fn delete_group_with_stickers_keeps_user_files_in_folder() {
        let conn = test_conn();
        let (root, db_path) = tmp_ws("group-del-keep");
        let g = crate::db::group_repo::create(&conn, "待删组", None).unwrap();
        let id = create_sticker(
            &conn,
            &sticker_repo::NewSticker {
                group_id: Some(g.id),
                title: "组内便签".into(),
                content: "# X".into(),
                ..Default::default()
            },
            &db_path,
        )
        .unwrap();

        let dir = root.join("stickers").join("待删组");
        std::fs::write(dir.join("附件说明.txt"), "用户自己放的").unwrap();
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::write(dir.join(".git").join("config"), "x").unwrap();

        let n = delete_group(&conn, g.id, "with-stickers", &db_path).unwrap();
        assert_eq!(n, 1, "只算真正被删掉的便签");
        assert!(!dir.join("组内便签.md").exists(), "便签 md 该被删除");
        assert!(dir.join("附件说明.txt").is_file(), "用户自己的文件必须留下");
        assert!(dir.join(".git").join("config").is_file(), "隐藏内容必须留下");
        assert!(get_sticker(&conn, id, &db_path).unwrap().is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// to-default 搬回便签根目录时若与未分组的同名便签撞车，必须**整单拒绝**并提示改名：
    /// 先搬过去的话两份 md 会互相顶掉，最后连便签带附件一起消失。
    #[test]
    fn delete_group_to_default_refuses_title_conflict() {
        let conn = test_conn();
        let (root, db_path) = tmp_ws("group-del-conflict");
        let loose = create_sticker(
            &conn,
            &sticker_repo::NewSticker {
                title: "日志".into(),
                content: "未分组的".into(),
                ..Default::default()
            },
            &db_path,
        )
        .unwrap();
        // 跨分组重名是允许的：分组里的那张直接建在分组目录下
        let g = crate::db::group_repo::create(&conn, "工作", None).unwrap();
        let inner = create_sticker(
            &conn,
            &sticker_repo::NewSticker {
                group_id: Some(g.id),
                title: "日志".into(),
                content: "分组里的".into(),
                ..Default::default()
            },
            &db_path,
        )
        .unwrap();
        let inner_file = root.join("stickers").join("工作").join("日志.md");
        assert!(inner_file.is_file(), "前置：分组里的 md 已就位");

        let err = delete_group(&conn, g.id, "to-default", &db_path).unwrap_err().to_string();
        assert!(err.contains("同名"), "应提示改名：{err}");
        assert!(crate::db::group_repo::get(&conn, g.id).unwrap().is_some(), "分组必须还在");
        assert!(inner_file.is_file(), "分组里的 md 应留在原位");
        assert!(root.join("stickers").join("日志.md").is_file(), "未分组那份也没动");
        assert_eq!(get_sticker(&conn, loose, &db_path).unwrap().unwrap().content, "未分组的");
        assert_eq!(get_sticker(&conn, inner, &db_path).unwrap().unwrap().content, "分组里的");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// to-default 的落盘侧冲突：便签根目录里有一个同名文件、但库里没有对应便签
    /// （用户刚拖进来、同步还没认领）——也要在删行/搬文件之前整单拒绝。
    #[test]
    fn delete_group_to_default_refuses_unclaimed_file_conflict() {
        let conn = test_conn();
        let (root, db_path) = tmp_ws("group-del-stray");
        let g = crate::db::group_repo::create(&conn, "工作", None).unwrap();
        let inner = create_sticker(
            &conn,
            &sticker_repo::NewSticker {
                group_id: Some(g.id),
                title: "日志".into(),
                content: "分组里的".into(),
                ..Default::default()
            },
            &db_path,
        )
        .unwrap();
        // 根目录直接放一个同名 md（不建 DB 行，模拟"还没被同步认领的文件"）
        std::fs::write(root.join("stickers").join("日志.md"), "孤儿文件").unwrap();

        let err = delete_group(&conn, g.id, "to-default", &db_path).unwrap_err().to_string();
        assert!(err.contains("已有文件"), "应提示根目录已有同名文件：{err}");
        assert!(crate::db::group_repo::get(&conn, g.id).unwrap().is_some(), "分组必须还在");
        assert!(
            root.join("stickers").join("工作").join("日志.md").is_file(),
            "组内便签不该被搬走"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("stickers").join("日志.md")).unwrap(),
            "孤儿文件",
            "用户的文件不该被动"
        );
        assert_eq!(get_sticker(&conn, inner, &db_path).unwrap().unwrap().content, "分组里的");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 改标题只有大小写不同（`Note` → `note`）：Windows 下就是同一个文件，
    /// 不能"删掉目标再搬"把自己弄没。
    #[test]
    fn update_title_with_case_only_difference_keeps_md() {
        let conn = test_conn();
        let (root, db_path) = tmp_ws("case-title");
        let id = create_sticker(
            &conn,
            &sticker_repo::NewSticker {
                title: "Note".into(),
                content: "正文".into(),
                ..Default::default()
            },
            &db_path,
        )
        .unwrap();

        update_sticker(
            &conn,
            id,
            &sticker_repo::StickerPatch { title: Some("note".into()), ..Default::default() },
            &db_path,
        )
        .unwrap();

        let file = root.join("stickers").join("note.md");
        assert!(file.is_file(), "md 不能消失（Windows 大小写不敏感，路径解析到同一个文件）");
        let s = get_sticker(&conn, id, &db_path).unwrap().unwrap();
        assert_eq!(s.title, "note");
        assert_eq!(s.content, "正文");
        let _ = std::fs::remove_dir_all(&root);
    }
}
