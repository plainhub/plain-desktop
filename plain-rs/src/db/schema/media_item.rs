use rusqlite::Connection;

pub(super) fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS media_item (
                media_type TEXT NOT NULL,
                media_id TEXT NOT NULL PRIMARY KEY,
                duration_ms INTEGER NOT NULL,
                updated_at TEXT NOT NULL
            );",
    )
}
