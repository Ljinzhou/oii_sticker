//! `sticker_repo` 提供对 `stickers` 表的同步 CRUD。
//!
//! 所有方法以 `&Connection` 为入参，由调用方（state.rs / commands.rs）
//! 负责把 DB IO 派发到 `spawn_blocking` 上运行，避免阻塞 UI 线程。

use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::models::Sticker;

const COLS: &str = "id, parent_id, title, content, heading_level,
                pos_x, pos_y, width, height, opacity, bg_color,
                always_on_top, auto_scroll, is_completed,
                group_id, display_mode, created_at, updated_at,
                window_hidden, uid, file_name";

/// 8 位短随机 id（小写字母+数字的十六进制形态，仅程序内部使用）。
///
/// 用途：窗口标识 / `assets/<uid>/` 目录 / 跨工作空间合并时避免撞号。
/// 生成方式：纳秒时间戳与计数器混合打散后取 32 位——不引入 rand 依赖，
/// 同进程内计数递增保证同毫秒不重复；数据库侧另有 UNIQUE 索引兜底。
pub fn new_uid() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    let mixed = nanos
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(COUNTER.fetch_add(1, Ordering::Relaxed).wrapping_mul(0xBF58_476D_1CE4_E5B9));
    format!("{:08x}", (mixed ^ (mixed >> 32)) as u32)
}

/// 新建一条便签，返回自增 id（同时生成 8 位随机 uid）。
///
/// `sort_order` 取该分组内 `MAX(sort_order) + 1`（追加到**组末尾**）：
/// 不能依赖列默认值 0，否则组内拖拽重排（sort_order 被重编号成 0..n）之后
/// 新建的便签会插到组首，与 "新便签追加在最后" 的预期不符。
pub fn insert(conn: &Connection, s: &NewSticker) -> Result<i64> {
    let bg = s.bg_color.as_deref();
    let order: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM stickers WHERE group_id IS ?1",
            params![s.group_id],
            |row| row.get(0),
        )
        .context("计算新便签组内序号失败")?;
    conn.execute(
        "INSERT INTO stickers
           (parent_id, group_id, title, content, heading_level,
            pos_x, pos_y, width, height, opacity, bg_color,
            always_on_top, auto_scroll, is_completed, display_mode, uid, sort_order)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                 ?11, ?12, ?13, 0, 'display', ?14, ?15)",
        params![
            s.parent_id,
            s.group_id,
            s.title,
            s.content,
            s.heading_level,
            s.pos_x,
            s.pos_y,
            s.width,
            s.height,
            s.opacity,
            bg,
            s.always_on_top as i32,
            s.auto_scroll as i32,
            new_uid(),
            order,
        ],
    )
    .context("插入便签失败")?;
    Ok(conn.last_insert_rowid())
}

/// 按 id 读取一条便签。
pub fn get(conn: &Connection, id: i64) -> Result<Option<Sticker>> {
    let mut stmt = conn.prepare_cached(&format!("SELECT {COLS} FROM stickers WHERE id = ?1"))?;
    let s = stmt.query_row(params![id], row_to_sticker).optional()?;
    Ok(s)
}

/// 列出全部便签（组内按用户拖拽顺序；`sort_order` 相同的旧数据按 id 兜底）。
pub fn list_all(conn: &Connection) -> Result<Vec<Sticker>> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT {COLS} FROM stickers ORDER BY sort_order, id"
    ))?;
    let rows = stmt
        .query_map([], row_to_sticker)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// 某一分组（`None` = 未分组）内的便签 id，按当前顺序。
pub fn ids_in_group(conn: &Connection, group_id: Option<i64>) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id FROM stickers WHERE group_id IS ?1 ORDER BY sort_order, id",
    )?;
    let rows = stmt
        .query_map(params![group_id], |r| r.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// 组内重排（主控台拖拽调整位置）：`ids` 必须是**同一分组**且覆盖该组全部便签的完整新顺序。
///
/// 与 `group_repo::reorder` 同一套防御：id 集合与库内不一致直接报错，
/// 避免前后端视图不同步时静默丢序。事务内把组内 `sort_order` 重编号为 0..n。
pub fn reorder(conn: &Connection, ids: &[i64]) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let first = get(conn, ids[0])?.context("便签不存在")?;
    let group_id = first.group_id;
    let mut existing = ids_in_group(conn, group_id)?;
    let mut wanted = ids.to_vec();
    existing.sort_unstable();
    wanted.sort_unstable();
    if existing != wanted {
        bail!("排序列表必须为该分组全部便签，且与当前顺序集合一致");
    }
    let tx = conn.unchecked_transaction()?;
    for (index, id) in ids.iter().enumerate() {
        tx.execute(
            "UPDATE stickers SET sort_order = ?1, updated_at = datetime('now') WHERE id = ?2",
            params![index as i64, id],
        )
        .context("更新便签排序失败")?;
    }
    tx.commit()?;
    Ok(())
}

/// 部分更新便签字段；只覆盖传入的 Option 字段。
pub fn update(conn: &Connection, id: i64, patch: &StickerPatch) -> Result<()> {
    conn.execute(
        "UPDATE stickers SET
            title        = COALESCE(?2, title),
            content      = COALESCE(?3, content),
            pos_x        = COALESCE(?4, pos_x),
            pos_y        = COALESCE(?5, pos_y),
            width        = COALESCE(?6, width),
            height       = COALESCE(?7, height),
            opacity      = COALESCE(?8, opacity),
            bg_color     = COALESCE(?9, bg_color),
            always_on_top= COALESCE(?10, always_on_top),
            auto_scroll  = COALESCE(?11, auto_scroll),
            is_completed = COALESCE(?12, is_completed),
            display_mode = COALESCE(?13, display_mode),
            file_name    = COALESCE(?14, file_name),
            updated_at   = datetime('now')
         WHERE id = ?1",
        params![
            id,
            patch.title,
            patch.content,
            patch.pos_x,
            patch.pos_y,
            patch.width,
            patch.height,
            patch.opacity,
            patch.bg_color,
            patch.always_on_top.map(|b| b as i32),
            patch.auto_scroll.map(|b| b as i32),
            patch.is_completed.map(|b| b as i32),
            patch.display_mode,
            patch.file_name,
        ],
    )
    .context("更新便签失败")?;
    Ok(())
}

/// 记录窗口隐藏状态（程序退出对隐藏窗口、隐藏命令调用）。
/// 只改 window_hidden，保留最近一次几何，便于重新显示时恢复位置。
pub fn update_window_hidden(conn: &Connection, id: i64, hidden: bool) -> Result<()> {
    conn.execute(
        "UPDATE stickers SET window_hidden = ?2, updated_at = datetime('now')
         WHERE id = ?1",
        params![id, hidden as i32],
    )
    .context("更新窗口隐藏状态失败")?;
    Ok(())
}

/// 记录窗口位置与尺寸（程序退出对显示窗口调用），并标记为显示态。
pub fn update_window_bounds(conn: &Connection, id: i64, x: i32, y: i32, w: i32, h: i32) -> Result<()> {
    conn.execute(
        "UPDATE stickers SET
            pos_x = ?2, pos_y = ?3, width = ?4, height = ?5,
            window_hidden = 0,
            updated_at = datetime('now')
         WHERE id = ?1",
        params![id, x, y, w, h],
    )
    .context("更新窗口几何失败")?;
    Ok(())
}

/// 删除一条便签，依赖外键 ON DELETE CASCADE 清理子树与关联记录。
pub fn delete(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM stickers WHERE id = ?1", params![id])
        .context("删除便签失败")?;
    Ok(())
}

fn row_to_sticker(row: &rusqlite::Row<'_>) -> rusqlite::Result<Sticker> {
    Ok(Sticker {
        id: row.get(0)?,
        parent_id: row.get(1)?,
        title: row.get(2)?,
        content: row.get(3)?,
        heading_level: row.get(4)?,
        pos_x: row.get(5)?,
        pos_y: row.get(6)?,
        width: row.get(7)?,
        height: row.get(8)?,
        opacity: row.get(9)?,
        bg_color: row.get(10)?,
        always_on_top: row.get(11)?,
        auto_scroll: row.get(12)?,
        is_completed: row.get(13)?,
        group_id: row.get(14)?,
        display_mode: row.get(15)?,
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
        window_hidden: row.get(18)?,
        uid: row.get(19)?,
        file_name: row.get(20)?,
    })
}

/// 新建便签入参。
/// `#[serde(default)]`：前端未传的字段（如 heading_level）用 Default，
/// 避免 invoke 报 "missing field"。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NewSticker {
    pub parent_id: Option<i64>,
    pub group_id: Option<i64>,
    pub title: String,
    pub content: String,
    pub heading_level: i32,
    pub pos_x: i32,
    pub pos_y: i32,
    pub width: i32,
    pub height: i32,
    pub opacity: f32,
    pub bg_color: Option<String>,
    pub always_on_top: bool,
    pub auto_scroll: bool,
}

/// 部分更新便签入参：None 表示不动该字段。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StickerPatch {
    pub title: Option<String>,
    pub content: Option<String>,
    pub pos_x: Option<i32>,
    pub pos_y: Option<i32>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub opacity: Option<f32>,
    pub bg_color: Option<String>,
    pub always_on_top: Option<bool>,
    pub auto_scroll: Option<bool>,
    pub is_completed: Option<bool>,
    pub display_mode: Option<String>,
    /// 该便签 md 文件相对 `stickers/` 的路径（分组迁移 / 重命名时写回）。
    pub file_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::connection::open;

    /// 前端可能不传可选字段（如 heading_level）：serde(default) 保证不报 missing field。
    #[test]
    fn new_sticker_missing_fields_deserializes() {
        let json = r##"{
            "title": "测试",
            "content": "正文",
            "pos_x": 10,
            "pos_y": 20,
            "width": 300,
            "height": 400,
            "opacity": 0.9,
            "bg_color": "#FFEEAA",
            "always_on_top": false,
            "auto_scroll": false
        }"##;
        let s: NewSticker = serde_json::from_str(json).expect("缺失 heading_level 等字段也应成功");
        assert_eq!(s.title, "测试");
        assert_eq!(s.heading_level, 0);
        assert_eq!(s.parent_id, None);
        assert_eq!(s.bg_color.as_deref(), Some("#FFEEAA"));
    }

    /// uid：8 位十六进制（小写字母+数字），连续生成不重复。
    #[test]
    fn new_uid_is_eight_hex_chars_and_unique() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..200 {
            let uid = new_uid();
            assert_eq!(uid.len(), 8, "长度应为 8：{uid}");
            assert!(
                uid.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
                "仅小写字母+数字：{uid}"
            );
            assert!(seen.insert(uid.clone()), "不得重复：{uid}");
        }
    }

    /// 每个用例一个独立库文件：并行执行时互不污染（同一文件会让计数类断言随机失败）。
    fn test_conn(tag: &str) -> Connection {
        // 用临时文件库测试（WAL 模式在内存库上可用但行为略有差异）
        let dir = std::env::temp_dir().join(format!("oii-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let conn = open(&dir.join("test.db")).unwrap();
        crate::db::schema::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn sticker_crud_roundtrip() {
        let conn = test_conn("crud");
        let new = NewSticker {
            title: "测试便签".into(),
            content: "# 标题
正文".into(),
            pos_x: 10,
            pos_y: 20,
            width: 300,
            height: 400,
            opacity: 0.85,
            bg_color: Some("#FFEEAA".into()),
            always_on_top: true,
            ..Default::default()
        };
        let id = insert(&conn, &new).unwrap();
        assert!(id > 0);

        let s = get(&conn, id).unwrap().expect("应能读到");
        assert_eq!(s.title, "测试便签");
        assert_eq!(s.display_mode, "display");
        assert!(s.always_on_top);
        assert_eq!(s.mode(), crate::models::StickerMode::Display);
        assert!(!s.window_hidden, "新便签默认显示");
        assert_eq!(s.uid.as_deref().map(str::len), Some(8), "新便签应有 8 位随机 uid");
        assert_eq!(s.file_name, None, "文件名由文件层决定");

        // file_name 可由迁移/重命名写回
        update(
            &conn,
            id,
            &StickerPatch { file_name: Some("学习/英语/测试.md".into()), ..Default::default() },
        )
        .unwrap();
        assert_eq!(
            get(&conn, id).unwrap().unwrap().file_name.as_deref(),
            Some("学习/英语/测试.md")
        );

        // 窗口状态：隐藏标记 + 几何记录
        update_window_hidden(&conn, id, true).unwrap();
        let s = get(&conn, id).unwrap().unwrap();
        assert!(s.window_hidden);
        assert_eq!((s.pos_x, s.pos_y, s.width, s.height), (10, 20, 300, 400));
        update_window_bounds(&conn, id, 88, 66, 320, 240).unwrap();
        let s = get(&conn, id).unwrap().unwrap();
        assert!(!s.window_hidden, "记录几何时视为显示");
        assert_eq!((s.pos_x, s.pos_y, s.width, s.height), (88, 66, 320, 240));

        // 部分更新
        update(
            &conn,
            id,
            &StickerPatch {
                title: Some("改名".into()),
                display_mode: Some("interact".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let s = get(&conn, id).unwrap().unwrap();
        assert_eq!(s.title, "改名");
        assert_eq!(s.mode(), crate::models::StickerMode::Interact);
        assert_eq!(s.content, "# 标题
正文"); // 未更新的字段保留

        // 列表
        let all = list_all(&conn).unwrap();
        assert_eq!(all.len(), 1);

        // 删除级联清理
        delete(&conn, id).unwrap();
        assert!(get(&conn, id).unwrap().is_none());
    }

    /// 组内排序（v21）：新便签追加到组末尾；拖拽重排按传入顺序重编号；
    /// 集合与库内不一致时拒绝（防前后端视图不同步时静默丢序）。
    #[test]
    fn sort_order_appends_and_reorders_within_group() {
        let conn = test_conn("sort-order");
        let group = crate::db::group_repo::create(&conn, "工作", None).unwrap();
        let make = |title: &str, gid: Option<i64>| {
            insert(
                &conn,
                &NewSticker { title: title.into(), group_id: gid, ..Default::default() },
            )
            .unwrap()
        };
        let a = make("甲", Some(group.id));
        let b = make("乙", Some(group.id));
        let loose = make("散", None);

        // 新便签追加到组末尾（不能因默认值 0 插到组首）
        assert_eq!(ids_in_group(&conn, Some(group.id)).unwrap(), vec![a, b]);
        assert_eq!(ids_in_group(&conn, None).unwrap(), vec![loose]);

        // 拖拽重排：乙在前、甲在后
        reorder(&conn, &[b, a]).unwrap();
        assert_eq!(ids_in_group(&conn, Some(group.id)).unwrap(), vec![b, a]);

        // 再新建 → 仍追加到末尾（旧 bug：sort_order 默认 0 会插到第 1 位之后）
        let c = make("丙", Some(group.id));
        assert_eq!(ids_in_group(&conn, Some(group.id)).unwrap(), vec![b, a, c]);

        // 集合不一致（少了 c / 混入其它组的 id）一律拒绝
        assert!(reorder(&conn, &[b, a]).is_err(), "必须覆盖该组全部便签");
        assert!(reorder(&conn, &[b, a, c, loose]).is_err(), "不得跨组重排");
    }
}
