use rusqlite::Connection;

pub(super) fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS app_files (
                id          TEXT PRIMARY KEY,
                size        INTEGER NOT NULL DEFAULT 0,
                mime_type   TEXT NOT NULL DEFAULT '',
                real_path   TEXT NOT NULL DEFAULT '',
                ref_count   INTEGER NOT NULL DEFAULT 1,
                weak_hash   TEXT NOT NULL DEFAULT '',
                created_at  TEXT NOT NULL DEFAULT '',
                updated_at  TEXT NOT NULL DEFAULT ''
            );
            CREATE INDEX IF NOT EXISTS idx_app_files_weak ON app_files(size, weak_hash);",
    )
}
