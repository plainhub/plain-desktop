//! The chat-domain SQLite store comes from the shared `plain_rs::chat`
//! crate (`local_chat.db`: chats / chat_channels / peers /
//! nearby_device_cache / app_files, same schema as plain-app's Room DB).
//!
//! The desktop additionally keeps its bookmark tables in the same
//! database file; those are desktop-local and stay here —
//! [`ensure_bookmark_tables`] creates them alongside the shared schema.

mod bookmark;

pub use bookmark::{
    DBookmark, DBookmarkGroup, delete_bookmark_group, delete_bookmarks, get_bookmark_by_id,
    get_bookmark_group_by_id, get_bookmark_groups, get_bookmarks, get_bookmarks_by_group_id,
    insert_bookmark, insert_bookmark_group, update_bookmark, update_bookmark_group,
};
pub use plain_rs::chat::db::{
    ChatDb, DAppFile, DChannel, DChat, DNearbyDeviceCache, DPeer, iso_from_unix_millis, now_iso,
    now_millis, short_id,
};

/// Create the desktop-local bookmark tables in the shared chat database.
/// Idempotent — call once right after `ChatDb::open`.
pub fn ensure_bookmark_tables(db: &ChatDb) {
    db.with_conn(|conn| {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS bookmarks (
                id              TEXT PRIMARY KEY,
                url             TEXT NOT NULL DEFAULT '',
                title           TEXT NOT NULL DEFAULT '',
                favicon_path    TEXT NOT NULL DEFAULT '',
                group_id        TEXT NOT NULL DEFAULT '',
                pinned          INTEGER NOT NULL DEFAULT 0,
                click_count     INTEGER NOT NULL DEFAULT 0,
                last_clicked_at TEXT,
                sort_order      INTEGER NOT NULL DEFAULT 0,
                created_at      TEXT NOT NULL DEFAULT '',
                updated_at      TEXT NOT NULL DEFAULT ''
            );
            CREATE INDEX IF NOT EXISTS idx_bookmarks_group_id ON bookmarks(group_id);
            CREATE TABLE IF NOT EXISTS bookmark_groups (
                id         TEXT PRIMARY KEY,
                name       TEXT NOT NULL DEFAULT '',
                collapsed  INTEGER NOT NULL DEFAULT 0,
                sort_order INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT '',
                updated_at TEXT NOT NULL DEFAULT ''
            );",
        )
        .expect("create bookmark tables");
    });
}
