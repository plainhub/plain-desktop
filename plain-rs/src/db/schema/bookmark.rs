use rusqlite::Connection;

pub(super) fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS bookmarks (
                id TEXT NOT NULL, url TEXT NOT NULL, title TEXT NOT NULL,
                favicon_path TEXT NOT NULL, group_id TEXT NOT NULL,
                pinned INTEGER NOT NULL, click_count INTEGER NOT NULL,
                last_clicked_at TEXT, sort_order INTEGER NOT NULL,
                created_at TEXT NOT NULL, updated_at TEXT NOT NULL, PRIMARY KEY(id)
            );
            CREATE INDEX IF NOT EXISTS idx_bookmarks_group_id ON bookmarks(group_id);
            CREATE TABLE IF NOT EXISTS bookmark_groups (
                id TEXT NOT NULL, name TEXT NOT NULL,
                collapsed INTEGER NOT NULL, sort_order INTEGER NOT NULL,
                created_at TEXT NOT NULL, updated_at TEXT NOT NULL, PRIMARY KEY(id)
            );",
    )
}
