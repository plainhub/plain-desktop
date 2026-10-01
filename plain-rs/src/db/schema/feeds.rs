use rusqlite::Connection;

pub(super) fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS feeds (
                id TEXT NOT NULL PRIMARY KEY, name TEXT NOT NULL, url TEXT NOT NULL,
                logo TEXT NOT NULL DEFAULT '', fetch_content INTEGER NOT NULL, last_sync_at TEXT,
                last_error TEXT NOT NULL DEFAULT '', created_at TEXT NOT NULL, updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS feed_entries (
                id TEXT NOT NULL PRIMARY KEY, title TEXT NOT NULL, url TEXT NOT NULL, image TEXT NOT NULL,
                description TEXT NOT NULL, author TEXT NOT NULL, content TEXT NOT NULL, feed_id TEXT NOT NULL,
                raw_id TEXT NOT NULL, published_at TEXT NOT NULL, read INTEGER NOT NULL,
                created_at TEXT NOT NULL, updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_feed_entries_feed_id ON feed_entries(feed_id);
            CREATE INDEX IF NOT EXISTS idx_feed_entries_raw_id ON feed_entries(raw_id);
            CREATE UNIQUE INDEX IF NOT EXISTS idx_feeds_url ON feeds(url);",
    )
}
