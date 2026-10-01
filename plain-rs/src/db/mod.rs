use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};

#[cfg(feature = "sqlite_browse")]
pub mod browse;
#[cfg(feature = "chat")]
mod schema_chat;
#[cfg(feature = "library")]
mod schema_library;
#[cfg(feature = "chat")]
mod app_file;
#[cfg(feature = "chat")]
pub mod bookmark;
#[cfg(feature = "chat")]
mod channel;
#[cfg(feature = "chat")]
mod chat;
#[cfg(feature = "chat")]
mod nearby_device;
#[cfg(feature = "chat")]
mod peer;
#[cfg(feature = "chat")]
mod db_time;
#[cfg(feature = "library")]
pub mod audio_queue;
#[cfg(feature = "library")]
pub mod image_editor_project;
#[cfg(feature = "library")]
pub mod favorite_folder;
#[cfg(feature = "library")]
pub mod notes_feeds;
#[cfg(feature = "library")]
pub mod tag;
#[cfg(feature = "system")]
pub mod devtools;

#[cfg(feature = "chat")]
pub use app_file::DAppFile;
#[cfg(feature = "chat")]
pub use channel::DChannel;
#[cfg(feature = "chat")]
pub use chat::DChat;
#[cfg(feature = "chat")]
pub use nearby_device::DNearbyDeviceCache;
#[cfg(feature = "chat")]
pub use peer::DPeer;
#[cfg(feature = "chat")]
pub use db_time::{iso_from_unix_millis, now_iso, now_millis, short_id};
#[cfg(feature = "library")]
pub use audio_queue::{HISTORY_KEEP, PlayHistory, Playlist, PlaylistItem, QueueItem, QueueSource, QueueSourceKind};
#[cfg(feature = "library")]
pub use favorite_folder::FavoriteFolderRow;
#[cfg(feature = "library")]
pub use tag::{TagRelationRow, TagRow};
#[cfg(all(feature = "chat", feature = "sqlite_browse"))]
pub use crate::sqlite_browse::TableColumnMeta;

#[derive(Clone)]
pub struct Db(Arc<Mutex<Connection>>);

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
        #[cfg(feature = "chat")]
        Self::init_chat(&conn)?;
        #[cfg(feature = "library")]
        Self::init_library(&conn)?;
        Ok(Self(Arc::new(Mutex::new(conn))))
    }

    pub fn with_conn<F, T>(&self, f: F) -> T
    where
        F: FnOnce(&Connection) -> T,
    {
        let conn = self.0.lock().unwrap();
        f(&conn)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/db/mod.rs"]
mod tests;

#[cfg(all(test, feature = "library"))]
#[path = "../../tests/unit/library/db/mod.rs"]
mod library_tests;
#[cfg(all(test, feature = "chat"))]
#[path = "../../tests/unit/chat/db/mod.rs"]
mod chat_tests;
