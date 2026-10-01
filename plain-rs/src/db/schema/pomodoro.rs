use rusqlite::Connection;

pub(super) fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS pomodoro_items (
                id TEXT NOT NULL PRIMARY KEY,
                date TEXT NOT NULL,
                completed_count INTEGER NOT NULL,
                total_work_seconds INTEGER NOT NULL,
                total_break_seconds INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );",
    )
}
