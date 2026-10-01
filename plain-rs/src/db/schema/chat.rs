use rusqlite::Connection;

pub(super) fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS chats (
                id          TEXT PRIMARY KEY,
                from_id     TEXT NOT NULL DEFAULT '',
                to_id       TEXT NOT NULL DEFAULT '',
                channel_id  TEXT NOT NULL DEFAULT '',
                content     TEXT NOT NULL DEFAULT '{}',
                status      TEXT NOT NULL DEFAULT 'SENT',
                status_data TEXT NOT NULL DEFAULT '',
                created_at  TEXT NOT NULL DEFAULT '',
                updated_at  TEXT NOT NULL DEFAULT ''
            );
            CREATE INDEX IF NOT EXISTS idx_chats_from_id    ON chats(from_id);
            CREATE INDEX IF NOT EXISTS idx_chats_to_id      ON chats(to_id);
            CREATE INDEX IF NOT EXISTS idx_chats_channel_id ON chats(channel_id);
            CREATE TABLE IF NOT EXISTS chat_channels (
                id         TEXT PRIMARY KEY,
                name       TEXT NOT NULL DEFAULT '',
                owner_id   TEXT NOT NULL DEFAULT 'me',
                members    TEXT NOT NULL DEFAULT '[]',
                key        TEXT NOT NULL DEFAULT '',
                version    INTEGER NOT NULL DEFAULT 1,
                status     TEXT NOT NULL DEFAULT 'JOINED',
                created_at TEXT NOT NULL DEFAULT '',
                updated_at TEXT NOT NULL DEFAULT ''
            );",
    )
}
