use rusqlite::Connection;

pub(super) fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS feeds (
                id TEXT PRIMARY KEY, name TEXT NOT NULL DEFAULT '', url TEXT NOT NULL UNIQUE,
                fetch_content INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS feed_entries (
                id TEXT PRIMARY KEY, feed_id TEXT NOT NULL, title TEXT NOT NULL DEFAULT '',
                url TEXT NOT NULL DEFAULT '', image TEXT NOT NULL DEFAULT '', description TEXT NOT NULL DEFAULT '',
                author TEXT NOT NULL DEFAULT '', content TEXT NOT NULL DEFAULT '', raw_id TEXT NOT NULL DEFAULT '',
                published_at TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_feed_entries_raw ON feed_entries(feed_id,raw_id);
            CREATE INDEX IF NOT EXISTS idx_feed_entries_published ON feed_entries(published_at DESC);",
    )
}
