use rusqlite::Connection;

pub(super) fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS bookmarks (
                id TEXT PRIMARY KEY, url TEXT NOT NULL DEFAULT '', title TEXT NOT NULL DEFAULT '',
                favicon_path TEXT NOT NULL DEFAULT '', group_id TEXT NOT NULL DEFAULT '',
                pinned INTEGER NOT NULL DEFAULT 0, click_count INTEGER NOT NULL DEFAULT 0,
                last_clicked_at TEXT, sort_order INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT '', updated_at TEXT NOT NULL DEFAULT ''
            );
            CREATE INDEX IF NOT EXISTS idx_bookmarks_group_id ON bookmarks(group_id);
            CREATE TABLE IF NOT EXISTS bookmark_groups (
                id TEXT PRIMARY KEY, name TEXT NOT NULL DEFAULT '',
                collapsed INTEGER NOT NULL DEFAULT 0, sort_order INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT '', updated_at TEXT NOT NULL DEFAULT ''
            );",
    )
}
