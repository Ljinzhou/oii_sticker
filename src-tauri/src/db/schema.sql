-- === Oi Sticker SQLite Schema (v1) ===
-- 结构对齐旧项目 oi_sticker（保证旧 stickers.db 兼容），注释重写。
PRAGMA foreign_keys = ON;

-- 便签主表
CREATE TABLE stickers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    parent_id INTEGER REFERENCES stickers(id) ON DELETE CASCADE,
    title TEXT NOT NULL DEFAULT '',
    content TEXT NOT NULL DEFAULT '',
    heading_level INTEGER NOT NULL DEFAULT 0,
    pos_x INTEGER NOT NULL DEFAULT 100,
    pos_y INTEGER NOT NULL DEFAULT 100,
    width INTEGER NOT NULL DEFAULT 400,
    height INTEGER NOT NULL DEFAULT 500,
    opacity REAL    NOT NULL DEFAULT 0.90,
    bg_color TEXT,
    always_on_top INTEGER NOT NULL DEFAULT 0,
    auto_scroll INTEGER NOT NULL DEFAULT 0,
    is_completed INTEGER NOT NULL DEFAULT 0,
    alert_active INTEGER NOT NULL DEFAULT 0,
    display_mode TEXT NOT NULL DEFAULT 'display',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_stickers_parent ON stickers(parent_id);
CREATE INDEX idx_stickers_parent_level
    ON stickers(parent_id, heading_level);

-- 便签附加属性（提醒规则）
CREATE TABLE sticker_attrs (
    sticker_id INTEGER PRIMARY KEY REFERENCES stickers(id) ON DELETE CASCADE,
    due_date TEXT,
    remind_at TEXT,
    remind_rule TEXT,
    is_recurring INTEGER NOT NULL DEFAULT 0
);

-- 待办条目
CREATE TABLE todo_items (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    sticker_id        INTEGER NOT NULL REFERENCES stickers(id) ON DELETE CASCADE,
    child_sticker_id  INTEGER REFERENCES stickers(id) ON DELETE SET NULL,
    text              TEXT NOT NULL,
    done              INTEGER NOT NULL DEFAULT 0,
    completed_at      TEXT,
    sort_order        INTEGER NOT NULL DEFAULT 0,
    due_date          TEXT,
    remind_at         TEXT,
    remind_rule       TEXT,
    is_recurring      INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_todos_sticker_order ON todo_items(sticker_id, sort_order);
CREATE INDEX idx_todos_remind_due
    ON todo_items(done, remind_at)
    WHERE done = 0;
CREATE INDEX idx_todos_child ON todo_items(child_sticker_id);

-- 完成历史日志
CREATE TABLE completion_log (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    todo_item_id    INTEGER REFERENCES todo_items(id) ON DELETE SET NULL,
    sticker_id      INTEGER NOT NULL REFERENCES stickers(id) ON DELETE CASCADE,
    text            TEXT NOT NULL,      -- 形如 "正文 -> 写一篇800字作文"
    completed_at    TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_completion_sticker_time
    ON completion_log(sticker_id, completed_at DESC);
CREATE INDEX idx_completion_todo ON completion_log(todo_item_id);

-- 资源文件表
CREATE TABLE assets (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    sticker_id  INTEGER REFERENCES stickers(id) ON DELETE SET NULL,
    name        TEXT NOT NULL,           -- 原始文件名
    mime_type   TEXT NOT NULL,
    file_path   TEXT NOT NULL,           -- 相对 assets/ 子目录的路径
    file_size   INTEGER NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_assets_sticker ON assets(sticker_id);

-- 系统配置表
CREATE TABLE system_config (
    key          TEXT PRIMARY KEY,
    value        TEXT NOT NULL,
    description  TEXT NOT NULL DEFAULT '',
    updated_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 便签窗口个性化偏好（v2 引入）
CREATE TABLE sticker_prefs (
    sticker_id        INTEGER PRIMARY KEY REFERENCES stickers(id) ON DELETE CASCADE,
    opacity           REAL,
    title_centered    INTEGER,
    title_font_size   INTEGER,
    body_font_size    INTEGER,
    bg_color          TEXT,
    text_color        TEXT,
    auto_scroll_speed INTEGER
);

-- 提醒扫描索引（v5）：scheduler 每 10s 扫 remind_at 非空行。
CREATE INDEX idx_attrs_remind
    ON sticker_attrs(remind_at)
    WHERE remind_at IS NOT NULL;

-- 独立 Todo 块（v7）：与旧 todo_items 并存，供 <todo-block> 使用。
CREATE TABLE todo_blocks (
    id            TEXT PRIMARY KEY,
    sticker_id    INTEGER NOT NULL REFERENCES stickers(id) ON DELETE CASCADE,
    title         TEXT NOT NULL DEFAULT '',
    block_title   TEXT NOT NULL DEFAULT '',
    description   TEXT,
    is_completed  INTEGER NOT NULL DEFAULT 0,
    parent_id     TEXT REFERENCES todo_blocks(id) ON DELETE CASCADE,
    reminder_at   TEXT,
    due_at        TEXT,
    repeat_rule   TEXT,
    sort_order    INTEGER NOT NULL DEFAULT 0,
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at    TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_todo_blocks_parent ON todo_blocks(parent_id);
CREATE INDEX idx_todo_blocks_sticker ON todo_blocks(sticker_id);

-- 外部文件阅读历史（v11）：记事本替代功能预留
CREATE TABLE IF NOT EXISTS file_history (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    path        TEXT NOT NULL,
    name        TEXT NOT NULL,
    size        INTEGER NOT NULL DEFAULT 0,
    last_opened_at TEXT NOT NULL DEFAULT (datetime('now')),
    open_count  INTEGER NOT NULL DEFAULT 1,
    archived    INTEGER NOT NULL DEFAULT 0
);
