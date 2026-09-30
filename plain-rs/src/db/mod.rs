use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};

#[cfg(feature = "sqlite_browse")]
pub mod browse;
#[cfg(feature = "chat")]
pub mod chat;
#[cfg(feature = "nas")]
pub mod devtools;
#[cfg(feature = "library")]
pub mod library;

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
        #[cfg(feature = "library")]
        conn.execute_batch("CREATE TABLE IF NOT EXISTS db_migrations (key TEXT PRIMARY KEY);")?;
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

#[cfg(feature = "library")]
impl Db {
    pub fn import_legacy_library(&self, legacy_path: &Path) -> rusqlite::Result<()> {
        if !legacy_path.exists() {
            return Ok(());
        }
        if self.with_conn(|conn| conn.path().map(Path::new) == Some(legacy_path)) {
            return Ok(());
        }
        let mut conn = self.0.lock().unwrap();
        let migrated: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM db_migrations WHERE key = 'legacy_library')",
            [],
            |row| row.get(0),
        )?;
        if migrated {
            return Ok(());
        }
        conn.execute(
            "ATTACH DATABASE ?1 AS legacy_library",
            [legacy_path.to_string_lossy().as_ref()],
        )?;
        let result = (|| {
            let transaction = conn.transaction()?;
            for table in [
                "audio_queue_source",
                "audio_queue_items",
                "audio_playlists",
                "audio_playlist_items",
                "audio_play_history",
                "tags",
                "tag_relations",
                "favorite_folders",
                "library_prefs",
                "notes",
                "feeds",
                "feed_entries",
            ] {
                let present: bool = transaction.query_row(
                    "SELECT EXISTS(SELECT 1 FROM legacy_library.sqlite_master WHERE type = 'table' AND name = ?1)",
                    [table],
                    |row| row.get(0),
                )?;
                if !present {
                    continue;
                }
                let mut stmt =
                    transaction.prepare(&format!("PRAGMA legacy_library.table_info({table})"))?;
                let columns = stmt
                    .query_map([], |row| row.get::<_, String>(1))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                if columns.is_empty() {
                    continue;
                }
                let names = columns.join(", ");
                transaction.execute(
                    &format!("INSERT OR IGNORE INTO main.{table} ({names}) SELECT {names} FROM legacy_library.{table}"),
                    [],
                )?;
            }
            transaction.execute(
                "INSERT INTO db_migrations(key) VALUES ('legacy_library')",
                [],
            )?;
            transaction.commit()
        })();
        let detach = conn.execute_batch("DETACH DATABASE legacy_library;");
        result?;
        detach
    }
}

#[cfg(test)]
#[path = "../../tests/unit/db/mod.rs"]
mod tests;
