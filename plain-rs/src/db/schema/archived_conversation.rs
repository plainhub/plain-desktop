use rusqlite::Connection;

pub(super) fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS archived_conversations (
                conversation_id TEXT NOT NULL PRIMARY KEY,
                conversation_date TEXT NOT NULL
            );",
    )
}
