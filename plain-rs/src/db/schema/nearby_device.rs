use rusqlite::Connection;

pub(super) fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS nearby_device_cache (
                id          TEXT PRIMARY KEY,
                name        TEXT NOT NULL DEFAULT '',
                ips         TEXT NOT NULL DEFAULT '',
                port        INTEGER NOT NULL DEFAULT 0,
                device_type TEXT NOT NULL DEFAULT '',
                version     TEXT NOT NULL DEFAULT '',
                platform    TEXT NOT NULL DEFAULT '',
                last_seen   INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_nearby_device_cache_last_seen ON nearby_device_cache(last_seen DESC);",
    )
}
