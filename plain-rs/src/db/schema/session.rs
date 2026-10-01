use rusqlite::Connection;

pub(super) fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS sessions (
                client_id TEXT NOT NULL PRIMARY KEY,
                name TEXT NOT NULL DEFAULT '',
                type TEXT NOT NULL DEFAULT 'WEB',
                client_ip TEXT NOT NULL,
                os_name TEXT NOT NULL,
                os_version TEXT NOT NULL,
                browser_name TEXT NOT NULL,
                browser_version TEXT NOT NULL,
                token TEXT NOT NULL,
                last_active_at TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );",
    )
}
