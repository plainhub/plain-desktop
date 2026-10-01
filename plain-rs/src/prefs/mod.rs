//! Unified preferences storage for the Plain* apps — separate flat
//! string→JSON maps in `<data_dir>/system_prefs.json` and
//! `<data_dir>/user_prefs.json`, shared by plain-nas, plain-desktop,
//! and plain-app. System state and user-configurable settings have
//! independent files and key spaces.
//!
//! The whole map is kept in memory (it is a few KB) and every mutation
//! rewrites its file atomically (write a temporary sibling + rename), so
//! each file on disk is complete and pretty-printed for hand editing.
//! Every host shares one `Arc<Prefs>` per process — there is exactly one
//! writer, no stale caches. Media rows / sessions / events live in each
//! app's own store; the user library (audio queue/playlists/history,
//! tags, favorite folders, chat) lives in SQLite.

pub mod dlna;
pub mod identity;
pub mod server;

pub use identity::{AppIdentity, ensure_identity, ensure_mdns_hostname, ensure_url_token};

use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Map, Value};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};

static GLOBAL: OnceLock<Arc<Prefs>> = OnceLock::new();

/// Install the process-wide preferences (called once by the host shell
/// right after loading). Panics if called twice.
pub fn set_global(prefs: Arc<Prefs>) {
    if GLOBAL.set(prefs).is_err() {
        panic!("prefs::set_global called twice");
    }
}

/// Returns the process-wide preferences. Panics if `set_global` was not
/// called yet — same contract as `media::kv::get_default`.
pub fn get_default() -> &'static Prefs {
    GLOBAL
        .get()
        .expect("prefs::set_global must be called before get_default")
        .as_ref()
}

/// Non-panicking variant for background paths that may run before (or
/// without) the global being installed — e.g. the automount watcher.
pub fn try_get_default() -> Option<Arc<Prefs>> {
    GLOBAL.get().cloned()
}

/// Prefs failures carry the file path so a hand-edit mistake or disk
/// error is loud and actionable. Implements `std::error::Error`, so
/// `?` converts into the hosts' anyhow / GraphQL error types.
#[derive(Debug)]
pub enum PrefsError {
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    Encode(serde_json::Error),
    /// Deserialize of a stored value into the requested type failed.
    Decode(serde_json::Error),
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl fmt::Display for PrefsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrefsError::Read { path, source } => {
                write!(f, "read prefs {}: {source}", path.display())
            }
            PrefsError::Parse { path, source } => {
                write!(f, "parse prefs {}: {source}", path.display())
            }
            PrefsError::Encode(source) => write!(f, "encode prefs: {source}"),
            PrefsError::Decode(source) => write!(f, "decode prefs value: {source}"),
            PrefsError::Write { path, source } => {
                write!(f, "write prefs {}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for PrefsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PrefsError::Read { source, .. } | PrefsError::Write { source, .. } => Some(source),
            PrefsError::Parse { source, .. }
            | PrefsError::Encode(source)
            | PrefsError::Decode(source) => Some(source),
        }
    }
}

pub type Result<T> = std::result::Result<T, PrefsError>;

pub struct Prefs {
    system: PrefsStore,
    user: PrefsStore,
}

impl std::fmt::Debug for Prefs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Prefs")
            .field("system_path", &self.system.path)
            .field("user_path", &self.user.path)
            .finish()
    }
}

/// Location of the preferences file inside the app data dir.
pub fn default_path(data_dir: &Path) -> PathBuf {
    data_dir.join("system_prefs.json")
}

impl Prefs {
    /// Load the preferences at `path`. A missing file starts empty; a
    /// malformed file is an error (the file is hand-editable, so parse
    /// failures should be loud, not silently discarded).
    pub fn load(path: &Path) -> Result<Self> {
        let user_path = path.with_file_name("user_prefs.json");
        Self::load_pair(path, &user_path)
    }

    /// Load the system and user preference files independently.
    pub fn load_pair(system_path: &Path, user_path: &Path) -> Result<Self> {
        Ok(Self {
            system: PrefsStore::load(system_path)?,
            user: PrefsStore::load(user_path)?,
        })
    }

    /// Absolute path of the system preferences file.
    pub fn path(&self) -> &Path {
        &self.system.path
    }

    /// Absolute path of the user preferences file.
    pub fn user_path(&self) -> &Path {
        &self.user.path
    }

    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        self.system.get(key)
    }

    pub fn get_or<T: DeserializeOwned>(&self, key: &str, default: T) -> T {
        self.system.get_or(key, default)
    }

    pub fn set<T: Serialize>(&self, key: &str, value: T) -> Result<bool> {
        self.system.set(key, value)
    }

    pub fn remove(&self, key: &str) -> Result<bool> {
        self.system.remove(key)
    }

    pub fn clear(&self) -> Result<()> {
        self.system.clear()
    }

    pub fn entries(&self) -> Vec<(String, Value)> {
        self.system.entries()
    }

    pub fn entries_sorted(&self) -> Vec<(String, String)> {
        self.system.entries_sorted()
    }

    pub fn get_user<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        self.user.get(key)
    }

    pub fn get_user_or<T: DeserializeOwned>(&self, key: &str, default: T) -> T {
        self.user.get_or(key, default)
    }

    pub fn set_user<T: Serialize>(&self, key: &str, value: T) -> Result<bool> {
        self.user.set(key, value)
    }

    pub fn remove_user(&self, key: &str) -> Result<bool> {
        self.user.remove(key)
    }

    pub fn clear_user(&self) -> Result<()> {
        self.user.clear().map(|_| ())
    }

    pub fn user_entries(&self) -> Vec<(String, Value)> {
        self.user.entries()
    }

    pub fn user_entries_sorted(&self) -> Vec<(String, String)> {
        self.user.entries_sorted()
    }
}

struct PrefsStore {
    path: PathBuf,
    inner: RwLock<Map<String, Value>>,
}

impl PrefsStore {
    fn load(path: &Path) -> Result<Self> {
        let inner = match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).map_err(|source| PrefsError::Parse {
                path: path.to_path_buf(),
                source,
            })?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Map::new(),
            Err(source) => {
                return Err(PrefsError::Read {
                    path: path.to_path_buf(),
                    source,
                });
            }
        };
        Ok(Self {
            path: path.to_path_buf(),
            inner: RwLock::new(inner),
        })
    }

    /// Read one entry, deserialized from its JSON value.
    fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        match self.inner.read().unwrap().get(key) {
            None => Ok(None),
            Some(v) => Ok(Some(
                serde_json::from_value(v.clone()).map_err(PrefsError::Decode)?,
            )),
        }
    }

    /// Read one entry, or `default` when absent / not deserializable.
    fn get_or<T: DeserializeOwned>(&self, key: &str, default: T) -> T {
        self.get(key).unwrap_or(None).unwrap_or(default)
    }

    /// Write one entry and persist the file. Returns whether the value
    /// changed (skips the disk write when it did not).
    fn set<T: Serialize>(&self, key: &str, value: T) -> Result<bool> {
        let new = serde_json::to_value(value).map_err(PrefsError::Encode)?;
        let mut inner = self.inner.write().unwrap();
        if inner.get(key) == Some(&new) {
            return Ok(false);
        }
        inner.insert(key.to_string(), new);
        self.save(&inner)
    }

    /// Remove one entry (if present) and persist the file.
    fn remove(&self, key: &str) -> Result<bool> {
        let mut inner = self.inner.write().unwrap();
        if inner.remove(key).is_none() {
            return Ok(false);
        }
        self.save(&inner)
    }

    /// Remove every entry and persist the file.
    fn clear(&self) -> Result<()> {
        let mut inner = self.inner.write().unwrap();
        inner.clear();
        self.save(&inner)?;
        Ok(())
    }

    /// Every entry sorted by key with its raw JSON value.
    fn entries(&self) -> Vec<(String, Value)> {
        let mut entries: Vec<(String, Value)> = self
            .inner
            .read()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries
    }

    /// Every entry sorted by key, with values rendered as compact JSON.
    fn entries_sorted(&self) -> Vec<(String, String)> {
        self.entries()
            .into_iter()
            .map(|(k, v)| (k, v.to_string()))
            .collect()
    }

    fn save(&self, inner: &Map<String, Value>) -> Result<bool> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| PrefsError::Write {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(
            &tmp,
            serde_json::to_string_pretty(inner).map_err(PrefsError::Encode)?,
        )
        .map_err(|source| PrefsError::Write {
            path: tmp.clone(),
            source,
        })?;
        std::fs::rename(&tmp, &self.path).map_err(|source| PrefsError::Write {
            path: self.path.clone(),
            source,
        })?;
        Ok(true)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/prefs.rs"]
mod tests;
