//! Unified preferences storage for the Plain* apps — one flat
//! string→JSON map in `<data_dir>/prefs.json`, shared by plain-nas
//! (server settings, device identity, small app state) and the
//! plain-desktop Tauri shells (previously tauri-plugin-store). The key
//! names match plain-app's Jetpack DataStore entries, so the file is the
//! cross-platform contract.
//!
//! The whole map is kept in memory (it is a few KB) and every mutation
//! rewrites the file atomically (write `prefs.json.tmp` + rename), so the
//! file on disk is always complete and pretty-printed for hand editing.
//! Every host shares one `Arc<Prefs>` per process — there is exactly one
//! writer, no stale caches. Media rows / sessions / events live in each
//! app's own store; the user library (audio queue/playlists/history,
//! tags, favorite folders, chat) lives in SQLite.

pub mod dlna;
pub mod identity;

pub use identity::{AppIdentity, ensure_identity, ensure_mdns_hostname, ensure_url_token};

use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Map, Value};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

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
    path: PathBuf,
    inner: RwLock<Map<String, Value>>,
}

impl std::fmt::Debug for Prefs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Prefs").field("path", &self.path).finish()
    }
}

/// Location of the preferences file inside the app data dir.
pub fn default_path(data_dir: &Path) -> PathBuf {
    data_dir.join("prefs.json")
}

impl Prefs {
    /// Load the preferences at `path`. A missing file starts empty; a
    /// malformed file is an error (the file is hand-editable, so parse
    /// failures should be loud, not silently discarded).
    pub fn load(path: &Path) -> Result<Self> {
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

    /// Absolute path of the backing file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Read one entry, deserialized from its JSON value.
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        match self.inner.read().unwrap().get(key) {
            None => Ok(None),
            Some(v) => Ok(Some(
                serde_json::from_value(v.clone()).map_err(PrefsError::Decode)?,
            )),
        }
    }

    /// Read one entry, or `default` when absent / not deserializable.
    pub fn get_or<T: DeserializeOwned>(&self, key: &str, default: T) -> T {
        self.get(key).unwrap_or(None).unwrap_or(default)
    }

    /// Write one entry and persist the file. Returns whether the value
    /// changed (skips the disk write when it did not).
    pub fn set<T: Serialize>(&self, key: &str, value: T) -> Result<bool> {
        let new = serde_json::to_value(value).map_err(PrefsError::Encode)?;
        let mut inner = self.inner.write().unwrap();
        if inner.get(key) == Some(&new) {
            return Ok(false);
        }
        inner.insert(key.to_string(), new);
        self.save(&inner)
    }

    /// Remove one entry (if present) and persist the file.
    pub fn remove(&self, key: &str) -> Result<bool> {
        let mut inner = self.inner.write().unwrap();
        if inner.remove(key).is_none() {
            return Ok(false);
        }
        self.save(&inner)
    }

    /// Remove every entry and persist the file.
    pub fn clear(&self) -> Result<()> {
        let mut inner = self.inner.write().unwrap();
        inner.clear();
        self.save(&inner)?;
        Ok(())
    }

    /// Every entry sorted by key with its raw JSON value.
    pub fn entries(&self) -> Vec<(String, Value)> {
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

    /// Every entry sorted by key, values rendered as compact JSON —
    /// the `dataStoreEntries` GraphQL shape (plain-nas and plain-desktop
    /// render `serde_json::Value::to_string()` the same way).
    pub fn entries_sorted(&self) -> Vec<(String, String)> {
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
