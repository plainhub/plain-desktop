use rusqlite::Connection;

pub(super) fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS peers (
                id TEXT NOT NULL PRIMARY KEY,
                name TEXT NOT NULL,
                ip TEXT NOT NULL,
                key TEXT NOT NULL,
                public_key TEXT NOT NULL,
                status TEXT NOT NULL,
                port INTEGER NOT NULL,
                device_type TEXT NOT NULL,
                token TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );",
    )
}
