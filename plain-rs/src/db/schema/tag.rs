use rusqlite::Connection;

pub(super) fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS tags (
                id   TEXT PRIMARY KEY,
                type INTEGER NOT NULL DEFAULT 0,
                name TEXT    NOT NULL DEFAULT ''
            );
            CREATE TABLE IF NOT EXISTS tag_relations (
                tag_id TEXT NOT NULL DEFAULT '',
                key    TEXT NOT NULL DEFAULT '',
                PRIMARY KEY (tag_id, key)
            );
            CREATE INDEX IF NOT EXISTS idx_tag_relations_key ON tag_relations(key);",
    )
}
