//! Process-global media paths (data dir + cache dir) for the media
//! stack. Hosts install once at startup via [`set`] (plain-nas with its
//! `AppPaths::detect()` values, plain-desktop with its app-data dir).
//! Before that, [`detect`] falls back to the `PLAIN_RS_DATA_DIR` /
//! `PLAIN_RS_CACHE_DIR` env vars, then `~/.plainrs` defaults — the same
//! shape plain-nas's `consts::AppPaths` had, so ported tests keep a
//! seam.

use std::path::PathBuf;
use std::sync::OnceLock;

#[derive(Clone)]
pub struct MediaPaths {
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
}

static OVERRIDE: OnceLock<MediaPaths> = OnceLock::new();

/// Install the process-wide media paths. First call wins — later calls
/// are ignored, so tests in a host binary cannot fight the host's
/// startup value.
pub fn set(data_dir: PathBuf, cache_dir: PathBuf) {
    let _ = OVERRIDE.set(MediaPaths {
        data_dir,
        cache_dir,
    });
}

pub fn detect() -> MediaPaths {
    if let Some(p) = OVERRIDE.get() {
        return p.clone();
    }
    let data_dir = std::env::var("PLAIN_RS_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| default_data_dir());
    let cache_dir = std::env::var("PLAIN_RS_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| default_cache_dir());
    MediaPaths {
        data_dir,
        cache_dir,
    }
}

fn home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

fn default_data_dir() -> PathBuf {
    home().join(".plainrs").join("data")
}

fn default_cache_dir() -> PathBuf {
    home().join(".plainrs").join("cache")
}

/// Test seam: pin `PLAIN_RS_DATA_DIR` for the whole test binary to a
/// fixed, never-deleted directory, mirroring plain-nas's
/// `AppPaths::pin_test_data_dir`. Tests that pointed this var at a
/// per-test `TempDir` poisoned the process-global media search index:
/// whichever value the env var held when the index first initialized
/// rooted the index there, and the owning test's `TempDir` cleanup then
/// deleted the index directory out from under every later test.
#[cfg(test)]
pub(crate) fn pin_test_data_dir() -> PathBuf {
    static PINNED: OnceLock<PathBuf> = OnceLock::new();
    PINNED
        .get_or_init(|| {
            let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-media-data-dir");
            std::fs::create_dir_all(&dir).expect("mkdir target/test-media-data-dir");
            // SAFETY: test-only; writes the same value on every call.
            unsafe { std::env::set_var("PLAIN_RS_DATA_DIR", &dir) };
            dir
        })
        .clone()
}

#[cfg(test)]
#[path = "../../tests/unit/media/paths.rs"]
mod tests;
