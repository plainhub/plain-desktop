use rusqlite::Connection;

pub(super) fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS audio_queue_source (
                id            INTEGER PRIMARY KEY CHECK (id = 1),
                source        TEXT    NOT NULL DEFAULT 'NONE',
                playlist_id   TEXT    NOT NULL DEFAULT '',
                current_path  TEXT    NOT NULL DEFAULT '',
                current_index INTEGER NOT NULL DEFAULT -1,
                sort_by       TEXT    NOT NULL DEFAULT ''
            );
            CREATE TABLE IF NOT EXISTS audio_queue_items (
                path          TEXT PRIMARY KEY,
                sort_order    INTEGER NOT NULL DEFAULT 0,
                title         TEXT    NOT NULL DEFAULT '',
                artist        TEXT    NOT NULL DEFAULT '',
                duration_secs INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS audio_playlists (
                id         TEXT PRIMARY KEY,
                name       TEXT    NOT NULL DEFAULT '',
                created_at TEXT    NOT NULL DEFAULT '',
                updated_at TEXT    NOT NULL DEFAULT ''
            );
            CREATE TABLE IF NOT EXISTS audio_playlist_items (
                id            TEXT PRIMARY KEY,
                playlist_id   TEXT    NOT NULL DEFAULT '',
                audio_path    TEXT    NOT NULL DEFAULT '',
                title         TEXT    NOT NULL DEFAULT '',
                artist        TEXT    NOT NULL DEFAULT '',
                duration_secs INTEGER NOT NULL DEFAULT 0,
                sort_order    INTEGER NOT NULL DEFAULT 0,
                added_at      TEXT    NOT NULL DEFAULT ''
            );
            CREATE INDEX IF NOT EXISTS idx_audio_playlist_items_playlist
                ON audio_playlist_items(playlist_id, sort_order);
            CREATE TABLE IF NOT EXISTS audio_play_history (
                path          TEXT PRIMARY KEY,
                title         TEXT    NOT NULL DEFAULT '',
                artist        TEXT    NOT NULL DEFAULT '',
                duration_secs INTEGER NOT NULL DEFAULT 0,
                play_count    INTEGER NOT NULL DEFAULT 0,
                played_at     TEXT    NOT NULL DEFAULT ''
            );
            CREATE INDEX IF NOT EXISTS idx_audio_play_history_played
                ON audio_play_history(played_at DESC);
            CREATE TABLE IF NOT EXISTS library_prefs (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL DEFAULT ''
            );",
    )
}
