use rusqlite::Connection;

pub(super) fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS nearby_device_cache (
                id TEXT NOT NULL PRIMARY KEY,
                name TEXT NOT NULL,
                ips TEXT NOT NULL,
                port INTEGER NOT NULL,
                device_type TEXT NOT NULL,
                version TEXT NOT NULL,
                platform TEXT NOT NULL,
                last_seen TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_nearby_device_cache_last_seen ON nearby_device_cache(last_seen DESC);",
    )
}
