//! SQLite-backed storage for the user library — the plain-app Room table
//! shapes (`audio_queue_source`, `audio_queue_items`, `audio_playlists`,
//! `audio_playlist_items`, `audio_play_history`, `tags`, `tag_relations`,
//! `favorite_folders`) plus a tiny `library_prefs` key-value table for the
//! audio play mode. Wraps a single Connection in `Arc<Mutex<>>`, same
//! sharing model as `chat::db::ChatDb`.

use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};

pub mod audio_queue;
pub mod favorite_folder;
pub mod tag;

pub use audio_queue::{
    HISTORY_KEEP, PlayHistory, Playlist, PlaylistItem, QueueItem, QueueSource, QueueSourceKind,
};
pub use favorite_folder::FavoriteFolderRow;
pub use tag::{TagRelationRow, TagRow};

pub struct LibraryDb(Arc<Mutex<Connection>>);

impl Clone for LibraryDb {
    fn clone(&self) -> Self {
        LibraryDb(Arc::clone(&self.0))
    }
}

impl LibraryDb {
    /// Execute a closure with read/write access to the underlying Connection.
    pub fn with_conn<F, T>(&self, f: F) -> T
    where
        F: FnOnce(&Connection) -> T,
    {
        let conn = self.0.lock().unwrap();
        f(&conn)
    }

    /// Open (or create) the library database at `db_path`, creating the
    /// parent directory and all tables when missing.
    pub fn open(db_path: &Path) -> rusqlite::Result<Self> {
        if let Some(parent) = db_path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|e| {
                rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error {
                        code: rusqlite::ffi::ErrorCode::CannotOpen,
                        extended_code: 0,
                    },
                    Some(format!(
                        "failed to create database parent dir {}: {e}",
                        parent.display()
                    )),
                )
            })?;
        }
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA busy_timeout=5000;",
        )?;
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
            );",
        )?;
        Ok(LibraryDb(Arc::new(Mutex::new(conn))))
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/library/db/mod.rs"]
mod tests;
