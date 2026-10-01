use rusqlite::Connection;

pub(super) fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS audio_queue_source (
                id INTEGER NOT NULL,
                source TEXT NOT NULL,
                playlist_id TEXT NOT NULL,
                current_path TEXT NOT NULL,
                current_index INTEGER NOT NULL,
                sort_by TEXT NOT NULL,
                PRIMARY KEY(id)
            );
            CREATE TABLE IF NOT EXISTS audio_queue_items (
                path TEXT NOT NULL,
                sort_order INTEGER NOT NULL,
                title TEXT NOT NULL,
                artist TEXT NOT NULL,
                duration_ms INTEGER NOT NULL,
                PRIMARY KEY(path)
            );
            CREATE INDEX IF NOT EXISTS idx_audio_queue_items_sort_order ON audio_queue_items(sort_order);
            CREATE TABLE IF NOT EXISTS audio_playlists (
                id TEXT NOT NULL,
                name TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY(id)
            );
            CREATE TABLE IF NOT EXISTS audio_playlist_items (
                id TEXT NOT NULL,
                playlist_id TEXT NOT NULL,
                audio_path TEXT NOT NULL,
                title TEXT NOT NULL,
                artist TEXT NOT NULL,
                album_id TEXT NOT NULL DEFAULT '',
                duration_ms INTEGER NOT NULL,
                sort_order INTEGER NOT NULL,
                added_at TEXT NOT NULL,
                PRIMARY KEY(id)
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_audio_playlist_items_unique ON audio_playlist_items(playlist_id, audio_path);
            CREATE INDEX IF NOT EXISTS idx_audio_playlist_items_playlist ON audio_playlist_items(playlist_id, sort_order);
            CREATE TABLE IF NOT EXISTS audio_play_history (
                path TEXT NOT NULL,
                title TEXT NOT NULL,
                artist TEXT NOT NULL,
                duration_ms INTEGER NOT NULL,
                play_count INTEGER NOT NULL DEFAULT 0,
                played_at TEXT NOT NULL,
                PRIMARY KEY(path)
            );
            CREATE INDEX IF NOT EXISTS idx_audio_play_history_played ON audio_play_history(played_at);",
    )
}
