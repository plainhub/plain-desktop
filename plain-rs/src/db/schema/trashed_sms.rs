use rusqlite::Connection;

pub(super) fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS trashed_sms (
                message_id TEXT NOT NULL PRIMARY KEY,
                is_mms INTEGER NOT NULL,
                trashed_at TEXT NOT NULL
            );",
    )
}
