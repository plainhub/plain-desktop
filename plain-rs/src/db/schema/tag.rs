use rusqlite::Connection;

pub(super) fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS tags (
                id TEXT NOT NULL PRIMARY KEY,
                name TEXT NOT NULL,
                type INTEGER NOT NULL,
                count INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS tag_relations (
                tag_id TEXT NOT NULL,
                key TEXT NOT NULL,
                type INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                size INTEGER NOT NULL,
                title TEXT NOT NULL,
                PRIMARY KEY (tag_id, key, type)
            );
            CREATE INDEX IF NOT EXISTS idx_tag_relations_key ON tag_relations(key);",
    )
}
