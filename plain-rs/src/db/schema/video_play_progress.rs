use rusqlite::Connection;

pub(super) fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS video_play_progress (
                media_id TEXT NOT NULL PRIMARY KEY,
                position_ms INTEGER NOT NULL,
                updated_at TEXT NOT NULL
            );",
    )
}
