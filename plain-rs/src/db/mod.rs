use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};

#[cfg(any(
    feature = "chat",
    feature = "library",
    feature = "sqlite_browse",
    feature = "sqlite_browse"
))]
mod managers;
#[cfg(feature = "chat")]
pub(crate) use managers::channel::{CHANNEL_COLS, row_to_channel};
#[cfg(feature = "chat")]
pub(crate) use managers::chat::{CHAT_COLS, row_to_chat};
#[cfg(feature = "chat")]
pub(crate) use managers::peer::{PEER_COLS, row_to_peer};
#[cfg(any(feature = "chat", feature = "library", feature = "sqlite_browse"))]
mod models;
#[cfg(any(feature = "chat", feature = "library"))]
mod schema;

#[cfg(feature = "sqlite_browse")]
pub use managers::browse;
#[cfg(feature = "sqlite_browse")]
pub use managers::devtools;
#[cfg(feature = "library")]
pub use managers::{audio_queue, favorite_folder, image_editor_project, notes_feeds, tag};
#[cfg(feature = "chat")]
pub use managers::{bookmark, chat_store};

#[cfg(all(feature = "chat", feature = "sqlite_browse"))]
pub use crate::sqlite_browse::TableColumnMeta;
#[cfg(feature = "chat")]
pub use managers::db_time::{iso_from_unix_millis, now_iso, now_millis, short_id};
#[cfg(feature = "library")]
pub use models::{
    ArchivedConversationRow, ClipboardRow, FavoriteFolderRow, HISTORY_KEEP, ImageEmbeddingRow,
    MediaItemRow, PlayHistory, Playlist, PlaylistItem, PomodoroItemRow, QueueItem, QueueSource,
    QueueSourceKind, SessionRow, ShareRow, TagRelationRow, TagRow, TrashedMessageRow,
    VideoPlayProgressRow,
};
#[cfg(feature = "chat")]
pub use models::{DAppFile, DChannel, DChat, DNearbyDeviceCache, DPeer};

#[derive(Clone)]
pub struct Db(Arc<Mutex<Connection>>, Arc<Mutex<()>>);

impl Db {
    pub fn open(db_path: &Path) -> rusqlite::Result<Self> {
        if let Some(parent) = db_path.parent().filter(|p| !p.as_os_str().is_empty()) {
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
        #[cfg(any(feature = "chat", feature = "library"))]
        schema::init(&conn)?;
        Ok(Self(Arc::new(Mutex::new(conn)), Arc::new(Mutex::new(()))))
    }

    #[cfg(feature = "chat")]
    pub(crate) fn app_files_lock(&self) -> std::io::Result<std::sync::MutexGuard<'_, ()>> {
        self.1
            .lock()
            .map_err(|_| std::io::Error::other("app file store lock poisoned"))
    }

    pub fn with_conn<F, T>(&self, f: F) -> T
    where
        F: FnOnce(&Connection) -> T,
    {
        let conn = self.0.lock().unwrap();
        f(&conn)
    }
}

#[cfg(feature = "chat")]
impl Db {
    pub fn table_columns(&self, table: &str) -> Vec<TableColumnMeta> {
        self.with_conn(|conn| crate::sqlite_browse::table_columns(conn, table))
    }

    pub fn primary_key_column(&self, table: &str) -> String {
        const FALLBACK: &str = "id";
        self.with_conn(|conn| {
            crate::sqlite_browse::primary_key_column(conn, table)
                .unwrap_or_else(|| FALLBACK.to_string())
        })
    }
}

#[cfg(test)]
#[path = "../../tests/unit/db/mod.rs"]
mod tests;

#[cfg(all(test, feature = "chat"))]
#[path = "../../tests/unit/chat/db/mod.rs"]
mod chat_tests;
#[cfg(all(test, feature = "library"))]
#[path = "../../tests/unit/library/db/mod.rs"]
mod library_tests;
