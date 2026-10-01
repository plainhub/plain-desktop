use rusqlite::Connection;

pub(super) fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS peers (
                id          TEXT PRIMARY KEY,
                name        TEXT NOT NULL DEFAULT '',
                ip          TEXT NOT NULL DEFAULT '',
                key         TEXT NOT NULL DEFAULT '',
                public_key  TEXT NOT NULL DEFAULT '',
                status      TEXT NOT NULL DEFAULT 'UNPAIRED',
                port        INTEGER NOT NULL DEFAULT 0,
                device_type TEXT NOT NULL DEFAULT '',
                token       TEXT NOT NULL DEFAULT '',
                created_at  TEXT NOT NULL DEFAULT '',
                updated_at  TEXT NOT NULL DEFAULT ''
            );",
    )
}
