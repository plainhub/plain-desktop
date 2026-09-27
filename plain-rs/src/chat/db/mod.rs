//! SQLite-backed storage for chat messages, channels, peers and
//! attachments — the same schema as plain-app's Room DB
//! (`local_chat.db`). Wraps a single Connection in `Arc<Mutex<>>` so it
//! can be shared across async runtimes without additional cloning.

use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};

mod app_file;
pub mod bookmark;
mod channel;
mod chat;
mod nearby_device;
mod peer;
mod utils;

pub use app_file::DAppFile;
pub use channel::DChannel;
pub use chat::DChat;
pub use nearby_device::DNearbyDeviceCache;
pub use peer::DPeer;
pub use utils::{iso_from_unix_millis, now_iso, now_millis, short_id};

// ---------------------------------------------------------------------------
// ChatDb — SQLite wrapper
// ---------------------------------------------------------------------------

pub struct ChatDb(Arc<Mutex<Connection>>);

impl Clone for ChatDb {
    fn clone(&self) -> Self {
        ChatDb(Arc::clone(&self.0))
    }
}

impl ChatDb {
    /// Execute a closure with read/write access to the underlying Connection.
    /// Used by debug GraphQL resolvers (db_tables, db_table_rows, etc.).
    pub fn with_conn<F, T>(&self, f: F) -> T
    where
        F: FnOnce(&Connection) -> T,
    {
        let conn = self.0.lock().unwrap();
        f(&conn)
    }

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
            );
            CREATE TABLE IF NOT EXISTS peers (
                id          TEXT PRIMARY KEY,
                name        TEXT NOT NULL DEFAULT '',
                ip          TEXT NOT NULL DEFAULT '',
                key         TEXT NOT NULL DEFAULT '',
                public_key  TEXT NOT NULL DEFAULT '',
                status      TEXT NOT NULL DEFAULT 'UNPAIRED',
                port        INTEGER NOT NULL DEFAULT 0,
                device_type TEXT NOT NULL DEFAULT '',
                token       TEXT NOT NULL DEFAULT '',
                created_at  TEXT NOT NULL DEFAULT '',
                updated_at  TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE IF NOT EXISTS nearby_device_cache (
                id          TEXT PRIMARY KEY,
                name        TEXT NOT NULL DEFAULT '',
                ips         TEXT NOT NULL DEFAULT '',
                port        INTEGER NOT NULL DEFAULT 0,
                device_type TEXT NOT NULL DEFAULT '',
                version     TEXT NOT NULL DEFAULT '',
                platform    TEXT NOT NULL DEFAULT '',
                last_seen   INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_nearby_device_cache_last_seen ON nearby_device_cache(last_seen DESC);
            CREATE TABLE IF NOT EXISTS app_files (
                id          TEXT PRIMARY KEY,
                size        INTEGER NOT NULL DEFAULT 0,
                mime_type   TEXT NOT NULL DEFAULT '',
                real_path   TEXT NOT NULL DEFAULT '',
                ref_count   INTEGER NOT NULL DEFAULT 1,
                weak_hash   TEXT NOT NULL DEFAULT '',
                created_at  TEXT NOT NULL DEFAULT '',
                updated_at  TEXT NOT NULL DEFAULT ''
            );
            CREATE INDEX IF NOT EXISTS idx_app_files_weak ON app_files(size, weak_hash);
            CREATE TABLE IF NOT EXISTS bookmarks (
                id TEXT PRIMARY KEY, url TEXT NOT NULL DEFAULT '', title TEXT NOT NULL DEFAULT '',
                favicon_path TEXT NOT NULL DEFAULT '', group_id TEXT NOT NULL DEFAULT '',
                pinned INTEGER NOT NULL DEFAULT 0, click_count INTEGER NOT NULL DEFAULT 0,
                last_clicked_at TEXT, sort_order INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT '', updated_at TEXT NOT NULL DEFAULT ''
            );
            CREATE INDEX IF NOT EXISTS idx_bookmarks_group_id ON bookmarks(group_id);
            CREATE TABLE IF NOT EXISTS bookmark_groups (
                id TEXT PRIMARY KEY, name TEXT NOT NULL DEFAULT '',
                collapsed INTEGER NOT NULL DEFAULT 0, sort_order INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT '', updated_at TEXT NOT NULL DEFAULT ''
            );",
        )?;
        Self::run_migrations(&conn)?;
        Ok(ChatDb(Arc::new(Mutex::new(conn))))
    }

    fn run_migrations(conn: &Connection) -> rusqlite::Result<()> {
        Self::ensure_column(conn, "chat_channels", "key", "TEXT NOT NULL DEFAULT ''")?;
        Self::ensure_column(conn, "peers", "token", "TEXT NOT NULL DEFAULT ''")?;
        Self::rename_column(conn, "chat_channels", "owner", "owner_id")?;
        Ok(())
    }

    /// Rename a column in place; no-op when the old name is already gone.
    /// `chat_channels.owner` → `owner_id` (2026-09-24 naming cleanup). The
    /// members JSON key rewrite (`"id":` → `"peerId":`) rides along — legacy
    /// `"id"` keys stay decodable via the serde alias.
    fn rename_column(conn: &Connection, table: &str, from: &str, to: &str) -> rusqlite::Result<()> {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let mut has_from = false;
        let mut has_to = false;
        stmt.query_map([], |row| row.get::<_, String>(1))?
            .filter_map(|r| r.ok())
            .for_each(|name| {
                if name == from {
                    has_from = true;
                }
                if name == to {
                    has_to = true;
                }
            });
        if has_from && !has_to {
            conn.execute(
                &format!("ALTER TABLE {table} RENAME COLUMN {from} TO {to}"),
                [],
            )?;
            conn.execute(
                "UPDATE chat_channels SET members = REPLACE(members, '\"id\":', '\"peerId\":') WHERE members LIKE '%\"id\"%'",
                [],
            )?;
        }
        Ok(())
    }

    fn ensure_column(
        conn: &Connection,
        table: &str,
        column: &str,
        definition: &str,
    ) -> rusqlite::Result<()> {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let exists = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(|r| r.ok())
            .any(|name| name == column);
        if !exists {
            conn.execute(
                &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
                [],
            )?;
        }
        Ok(())
    }

    /// Return the columns of `table` in declaration order, or an empty
    /// vec when the table is missing.
    /// Used by debug GraphQL resolvers (db_table_columns).
    pub fn table_columns(&self, table: &str) -> Vec<TableColumnMeta> {
        self.with_conn(|conn| crate::sqlite_browse::table_columns(conn, table))
    }

    /// Return the primary key column name for `table` (first column of a
    /// composite key), or `"id"` as a fallback when the table is missing
    /// or has no declared primary key.
    /// Used by debug GraphQL resolvers (db_table_info, delete_db_table_rows).
    pub fn primary_key_column(&self, table: &str) -> String {
        const FALLBACK: &str = "id";
        self.with_conn(|conn| {
            crate::sqlite_browse::primary_key_column(conn, table)
                .unwrap_or_else(|| FALLBACK.to_string())
        })
    }
}

pub use crate::sqlite_browse::TableColumnMeta;

#[cfg(test)]
#[path = "../../../tests/unit/chat/db/mod.rs"]
mod tests;
