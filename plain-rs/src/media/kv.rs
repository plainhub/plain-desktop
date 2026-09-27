//! Embedded KV store (fjall) replacing the Go `cockroachdb/pebble` usage.
//!
//! The whole app treats the database as one flat, byte-ordered key-value
//! namespace with key prefixes (`tag:`, `event:`, `session:`, ...), so we
//! expose exactly that surface backed by a single fjall keyspace and hide
//! the engine behind [`Db`].

use anyhow::{Context, Result};
use fjall::Slice;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

mod events;
mod keyvalue;
pub mod media_source_dirs;
mod password;
#[allow(dead_code)]
pub mod recent;
mod session;
mod signature_key;
pub mod storage_alias;
mod url_token;

pub use events::EventLog;
pub use keyvalue::{device_display_name, server_client_id};
pub use media_source_dirs as media_source;
pub use password::PasswordStore;
pub use session::{SessionInfo, SessionStore, token_key};
pub use signature_key::SignatureKey;
pub use storage_alias as storage;
pub use url_token::UrlToken;

/// Single flat keyspace holding the entire key namespace.
const KEYSPACE: &str = "default";

/// Thin wrapper around one fjall keyspace: `get`/`insert`/`remove`,
/// `scan_prefix`/`iter`, `batch`/`apply_batch`, `flush`.
#[derive(Clone)]
pub struct Db {
    database: fjall::Database,
    keyspace: fjall::Keyspace,
}

impl Db {
    /// Open (or create) a database at `path`. Not cached — use [`open`] for
    /// the process-wide instance (this raw form exists for tests).
    pub fn open(path: &Path) -> Result<Self> {
        std::fs::create_dir_all(path).with_context(|| format!("mkdir {}", path.display()))?;
        let database = fjall::Database::builder(path)
            .open()
            .with_context(|| format!("open fjall at {}", path.display()))?;
        let keyspace = database
            .keyspace(KEYSPACE, fjall::KeyspaceCreateOptions::default)
            .context("open fjall keyspace")?;
        Ok(Self { database, keyspace })
    }

    pub fn get<K: AsRef<[u8]>>(&self, key: K) -> Result<Option<Slice>> {
        Ok(self.keyspace.get(key.as_ref())?)
    }

    pub fn insert<K: AsRef<[u8]>, V: AsRef<[u8]>>(&self, key: K, value: V) -> Result<()> {
        self.keyspace
            .insert(key.as_ref(), value.as_ref())
            .context("fjall insert")?;
        Ok(())
    }

    pub fn remove<K: AsRef<[u8]>>(&self, key: K) -> Result<()> {
        self.keyspace.remove(key.as_ref()).context("fjall remove")?;
        Ok(())
    }

    /// Iterate all entries whose key starts with `prefix`, in key order.
    /// Items are fallible because values may need loading from disk.
    pub fn scan_prefix<'a>(
        &'a self,
        prefix: impl AsRef<[u8]> + 'a,
    ) -> impl Iterator<Item = Result<(Slice, Slice)>> + 'a {
        self.keyspace
            .prefix(prefix.as_ref())
            .map(|guard| guard.into_inner().map_err(Into::into))
    }

    /// Iterate every entry in the keyspace, in key order.
    pub fn iter(&self) -> impl Iterator<Item = Result<(Slice, Slice)>> + '_ {
        self.keyspace
            .iter()
            .map(|guard| guard.into_inner().map_err(Into::into))
    }

    /// Durably persist pending writes.
    pub fn flush(&self) -> Result<()> {
        self.database
            .persist(fjall::PersistMode::SyncData)
            .context("fjall persist")?;
        Ok(())
    }

    /// Start an atomic multi-key write batch; commit with [`Db::apply_batch`].
    pub fn batch(&self) -> Batch {
        self.batch_with_capacity(16)
    }

    /// Like [`Db::batch`] but pre-sized for `items` staged operations. Scan
    /// buffers stage thousands of ops between commits; pre-sizing avoids
    /// repeated internal growth on the hot path.
    pub fn batch_with_capacity(&self, items: usize) -> Batch {
        Batch {
            inner: fjall::OwnedWriteBatch::with_capacity(self.database.clone(), items),
            keyspace: self.keyspace.clone(),
        }
    }

    /// Atomically apply every operation staged in `batch`.
    pub fn apply_batch(&self, batch: Batch) -> Result<()> {
        batch.commit().context("fjall batch commit")?;
        Ok(())
    }
}

/// Staged atomic write batch. Create via [`Db::batch`], then apply.
pub struct Batch {
    inner: fjall::OwnedWriteBatch,
    keyspace: fjall::Keyspace,
}

impl Batch {
    pub fn insert(&mut self, key: impl AsRef<[u8]>, value: impl AsRef<[u8]>) {
        self.inner
            .insert(&self.keyspace, key.as_ref(), value.as_ref());
    }

    pub fn remove(&mut self, key: impl AsRef<[u8]>) {
        self.inner.remove(&self.keyspace, key.as_ref());
    }

    fn commit(self) -> Result<()> {
        Ok(self.inner.commit()?)
    }
}

static DB: OnceLock<Db> = OnceLock::new();

/// Open (or return the cached) default database.
pub fn open(path: &Path) -> Result<&'static Db> {
    if let Some(db) = DB.get() {
        return Ok(db);
    }
    let db = Db::open(path)?;
    Ok(DB.get_or_init(|| db))
}

/// Install an already-opened [`Db`] as the process-global default (first
/// caller wins; later calls return the incumbent). Hosts that open the
/// store themselves (tests with temp dirs, `MediaService::init`) use
/// this so `get_default()` callers share their handle.
pub fn set_default(db: Db) -> &'static Db {
    DB.get_or_init(move || db)
}

/// Returns the cached default database. Panics if `open` was not called yet.
pub fn get_default() -> &'static Db {
    DB.get()
        .expect("db::open must be called before get_default")
}

/// Non-panicking variant for background paths that may run before the
/// database is open — e.g. the automount watcher's event emission.
pub fn try_get_default() -> Option<&'static Db> {
    DB.get()
}

/// Returns the path of the default database directory.
pub fn default_db_path(data_dir: &Path) -> PathBuf {
    data_dir.join("fjall")
}

#[cfg(test)]
#[path = "../../tests/unit/media/kv.rs"]
mod tests;
