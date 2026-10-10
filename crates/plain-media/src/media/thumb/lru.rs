//! Hot-thumbnail in-memory LRU.
//!
//! File-cache hits still cost a disk read per request; on a grid that is
//! re-rendered repeatedly (scroll, tab switches) a small byte-budgeted LRU
//! turns the hot path into an Arc clone. Eviction is approximate LRU
//! (oldest `last_used` scanned out when over budget) — plenty for this
//! access pattern, and ~50 lines instead of an intrusive list.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

struct Entry {
    data: Arc<Vec<u8>>,
    last_used: Instant,
}

struct Inner {
    map: HashMap<PathBuf, Entry>,
    bytes: usize,
}

pub struct ThumbLru {
    cap_bytes: usize,
    cap_entries: usize,
    inner: Mutex<Inner>,
}

impl ThumbLru {
    pub fn new(cap_bytes: usize, cap_entries: usize) -> Self {
        ThumbLru {
            cap_bytes: cap_bytes.max(1),
            cap_entries: cap_entries.max(1),
            inner: Mutex::new(Inner {
                map: HashMap::new(),
                bytes: 0,
            }),
        }
    }

    pub fn get(&self, key: &PathBuf) -> Option<Arc<Vec<u8>>> {
        let mut inner = self.inner.lock().unwrap();
        let e = inner.map.get_mut(key)?;
        e.last_used = Instant::now();
        Some(e.data.clone())
    }

    pub fn put(&self, key: PathBuf, data: Arc<Vec<u8>>) {
        let mut inner = self.inner.lock().unwrap();
        // Replace-in-place keeps accounting simple.
        if let Some(old) = inner.map.insert(
            key.clone(),
            Entry {
                data: data.clone(),
                last_used: Instant::now(),
            },
        ) {
            inner.bytes = inner.bytes.saturating_sub(old.data.len());
        }
        inner.bytes += data.len();
        self.evict(&mut inner, &key);
    }

    /// Evict oldest-touched entries until both caps hold. The entry just
    /// inserted (`keep`) is never evicted by its own put.
    fn evict(&self, inner: &mut Inner, keep: &PathBuf) {
        while inner.map.len() > self.cap_entries || inner.bytes > self.cap_bytes {
            let Some(victim) = inner
                .map
                .iter()
                .filter(|(k, _)| *k != keep)
                .min_by_key(|(_, e)| e.last_used)
                .map(|(k, _)| k.clone())
            else {
                break; // only `keep` remains
            };
            if let Some(e) = inner.map.remove(&victim) {
                inner.bytes = inner.bytes.saturating_sub(e.data.len());
            }
        }
    }

    pub fn clear(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.map.clear();
        inner.bytes = 0;
    }

    #[cfg(test)]
    pub fn stats(&self) -> (usize, usize) {
        let inner = self.inner.lock().unwrap();
        (inner.map.len(), inner.bytes)
    }
}

static GLOBAL: std::sync::OnceLock<ThumbLru> = std::sync::OnceLock::new();

/// Configure the global LRU from `[thumbnails] lru_mb`. First call wins.
pub fn init_from_config(cfg: &crate::media::config::Config) {
    // Missing/invalid key (get_int → 0) falls back to the 32 MB default.
    let raw = cfg.get_int("thumbnails.lru_mb");
    let mb = if raw <= 0 { 32 } else { raw.clamp(1, 2048) } as usize;
    let _ = GLOBAL.set(ThumbLru::new(mb * 1024 * 1024, 4096));
}

pub fn global() -> &'static ThumbLru {
    GLOBAL.get_or_init(|| ThumbLru::new(32 * 1024 * 1024, 4096))
}

/// Test/bench hook: drop the hot cache.
#[doc(hidden)]
pub fn debug_clear() {
    global().clear();
}

#[cfg(test)]
#[path = "../../../tests/unit/media/thumb/lru.rs"]
mod tests;
