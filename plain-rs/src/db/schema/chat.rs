use rusqlite::Connection;

pub(super) fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS chats (
                id TEXT NOT NULL,
                from_id TEXT NOT NULL,
                to_id TEXT NOT NULL,
                channel_id TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'PENDING',
                status_data TEXT NOT NULL DEFAULT '',
                content TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY(id)
            );
            CREATE INDEX IF NOT EXISTS idx_chats_from_id ON chats(from_id);
            CREATE INDEX IF NOT EXISTS idx_chats_to_id ON chats(to_id);
            CREATE INDEX IF NOT EXISTS idx_chats_channel_id ON chats(channel_id);
            CREATE TABLE IF NOT EXISTS chat_channels (
                id TEXT NOT NULL,
                name TEXT NOT NULL,
                key TEXT NOT NULL,
                owner_id TEXT NOT NULL DEFAULT '',
                members TEXT NOT NULL,
                version INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL DEFAULT 'JOINED',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY(id)
            );",
    )
}
