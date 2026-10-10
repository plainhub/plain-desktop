//! Per-key request coalescing (single-flight).
//!
//! Grid UIs re-fire thumbnail requests in bursts (scroll, re-render,
//! retries). Without coalescing, N identical in-flight requests each burn
//! admission permits and CPU. With it, one request generates while the rest
//! wait on a keyed lock and then read the just-written cache.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::Mutex as AsyncMutex;

#[derive(Default)]
pub struct KeyedLocks {
    map: Mutex<HashMap<String, Arc<AsyncMutex<()>>>>,
}

impl KeyedLocks {
    pub fn new() -> Self {
        Self::default()
    }

    /// Run `f` under the key's lock. Concurrent callers with the same key
    /// serialize; different keys proceed independently. The map entry is
    /// removed once nobody waits on it, so the map stays proportional to
    /// in-flight work, not to the media library.
    pub async fn with_lock<T, F>(&self, key: String, f: F) -> T
    where
        F: std::future::Future<Output = T>,
    {
        let lock = {
            let mut map = self.map.lock().unwrap();
            map.entry(key.clone())
                .or_insert_with(|| Arc::new(AsyncMutex::new(())))
                .clone()
        };
        let lease = Lease {
            owner: self,
            key,
            lock,
        };
        let guard = lease.lock.clone().lock_owned().await;
        let out = f.await;
        drop(guard);
        drop(lease);
        out
    }

    fn maybe_remove(&self, key: &str, lock: &Arc<AsyncMutex<()>>) {
        let mut map = self.map.lock().unwrap();
        if let Some(cur) = map.get(key) {
            // The map's own entry plus our local clone account for exactly 2
            // references; any waiter holding a clone would make it ≥ 3, so
            // removing at 2 cannot strand anyone (a late waiter re-inserts).
            if Arc::ptr_eq(cur, lock) && Arc::strong_count(lock) == 2 {
                map.remove(key);
            }
        }
    }

    #[cfg(test)]
    pub fn in_flight_keys(&self) -> usize {
        self.map.lock().unwrap().len()
    }
}

struct Lease<'a> {
    owner: &'a KeyedLocks,
    key: String,
    lock: Arc<AsyncMutex<()>>,
}
impl Drop for Lease<'_> {
    fn drop(&mut self) {
        self.owner.maybe_remove(&self.key, &self.lock);
    }
}

#[cfg(test)]
#[path = "../../tests/unit/utils/keyed_locks.rs"]
mod tests;
