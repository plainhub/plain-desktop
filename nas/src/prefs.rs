//! Process-wide preferences glue over the shared `plain_rs::prefs`
//! engine (`<data_dir>/prefs.json`, one flat string→JSON map). The
//! engine itself — atomic tmp+rename writes, pretty-printing, key-sorted
//! entries — lives in plain-rs; this module only installs the
//! process-global instance and keeps the `crate::prefs::…` call sites
//! stable. User settings, device identity and small app state live here;
//! media rows / sessions / events live in the fjall store; the user
//! library (audio queue/playlists/history, tags, favorite folders,
//! chat) lives in SQLite via plain-rs.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

pub use plain_rs::prefs::Prefs;

static GLOBAL: OnceLock<Arc<Prefs>> = OnceLock::new();

/// Install the process-wide preferences (called once from `cmd::run`
/// right after loading). Panics if called twice.
pub fn set_global(prefs: Arc<Prefs>) {
    if GLOBAL.set(prefs).is_err() {
        panic!("prefs::set_global called twice");
    }
}

/// Returns the process-wide preferences. Panics if `set_global` was not
/// called yet — same contract as `db::get_default`.
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

/// Location of the preferences file inside the app data dir.
pub fn default_path(data_dir: &Path) -> PathBuf {
    plain_rs::prefs::default_path(data_dir)
}

#[cfg(test)]
#[path = "../tests/unit/prefs.rs"]
mod tests;
