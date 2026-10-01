use rusqlite::Connection;

pub(super) fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS app_files (
                id TEXT NOT NULL PRIMARY KEY,
                size INTEGER NOT NULL,
                mime_type TEXT NOT NULL,
                real_path TEXT NOT NULL,
                ref_count INTEGER NOT NULL,
                weak_hash TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_app_files_weak ON app_files(size, weak_hash);",
    )
}
