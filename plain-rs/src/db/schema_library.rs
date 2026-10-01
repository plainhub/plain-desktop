//! SQLite table definitions for the user library — the plain-app Room table
//! shapes (`audio_queue_source`, `audio_queue_items`, `audio_playlists`,
//! `audio_playlist_items`, `audio_play_history`, `tags`, `tag_relations`,
//! `favorite_folders`) plus a tiny `library_prefs` key-value table for the
//! audio play mode. Wraps a single Connection in `Arc<Mutex<>>`, same

use rusqlite::Connection;

impl crate::db::Db {
    pub(crate) fn init_library(conn: &Connection) -> rusqlite::Result<()> {
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
            CREATE TABLE IF NOT EXISTS tags (
                id   TEXT PRIMARY KEY,
                type INTEGER NOT NULL DEFAULT 0,
                name TEXT    NOT NULL DEFAULT ''
            );
            CREATE TABLE IF NOT EXISTS tag_relations (
                tag_id TEXT NOT NULL DEFAULT '',
                key    TEXT NOT NULL DEFAULT '',
                PRIMARY KEY (tag_id, key)
            );
            CREATE INDEX IF NOT EXISTS idx_tag_relations_key ON tag_relations(key);
            CREATE TABLE IF NOT EXISTS favorite_folders (
                root_path     TEXT NOT NULL,
                relative_path TEXT NOT NULL,
                alias         TEXT,
                PRIMARY KEY (root_path, relative_path)
            );
            CREATE TABLE IF NOT EXISTS library_prefs (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE IF NOT EXISTS image_editor_projects (
                id TEXT PRIMARY KEY,
                state_b64 TEXT NOT NULL DEFAULT '',
                thumbnail TEXT,
                canvas_width INTEGER NOT NULL DEFAULT 0,
                canvas_height INTEGER NOT NULL DEFAULT 0,
                layer_count INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_image_editor_projects_updated
                ON image_editor_projects(updated_at DESC);
            CREATE TABLE IF NOT EXISTS notes (
                id TEXT PRIMARY KEY, title TEXT NOT NULL DEFAULT '', content TEXT NOT NULL DEFAULT '',
                deleted_at TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS feeds (
                id TEXT PRIMARY KEY, name TEXT NOT NULL DEFAULT '', url TEXT NOT NULL UNIQUE,
                fetch_content INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS feed_entries (
                id TEXT PRIMARY KEY, feed_id TEXT NOT NULL, title TEXT NOT NULL DEFAULT '',
                url TEXT NOT NULL DEFAULT '', image TEXT NOT NULL DEFAULT '', description TEXT NOT NULL DEFAULT '',
                author TEXT NOT NULL DEFAULT '', content TEXT NOT NULL DEFAULT '', raw_id TEXT NOT NULL DEFAULT '',
                published_at TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_feed_entries_raw ON feed_entries(feed_id,raw_id);
            CREATE INDEX IF NOT EXISTS idx_feed_entries_published ON feed_entries(published_at DESC);
            CREATE INDEX IF NOT EXISTS idx_notes_updated ON notes(updated_at DESC);",
        )?;
        Ok(())
    }
}
