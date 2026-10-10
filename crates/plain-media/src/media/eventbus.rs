//! In-process pub/sub. The Go implementation uses a per-handler subscription
//! table backed by channels. We use a single shared bus and a small router
//! that supports the two event "shapes" the Go side uses:
//!   * `name(payload)` — broadcast only
//!   * `name(cid, payload)` — per-client broadcast
//!
//! Subscribers are looked up by a numeric handle that is returned from
//! `subscribe*` so callers can later call `unsubscribe`.
//!
//! The handler tables are tiny (single-digit entries on a busy install) and
//! publish happens at most a few times per second, so a single
//! `parking_lot::Mutex<HashMap>` is more than enough — `dashmap` would only
//! add an extra dep for zero observable benefit. Handlers are wrapped in
//! `Arc` so we can take a snapshot under the lock and invoke them after
//! dropping the lock (avoiding re-entrancy deadlocks if a handler
//! subscribes / unsubscribes).

use parking_lot::Mutex;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock as Lazy};

/// Media scan progress events (payload = scan progress JSON).
pub const EVENT_MEDIA_SCAN_PROGRESS: &str = "media:scan:progress";
/// File task (copy/move) progress events.
pub const EVENT_FILE_TASK_PROGRESS: &str = "file:task:progress";
/// DLNA renderer discovered on the LAN (published by the NAS sender
/// discovery loop).
pub const EVENT_DLNA_RENDERER_FOUND: &str = "dlna:renderer:found";
/// DLNA discovery sweep finished.
pub const EVENT_DLNA_DISCOVERY_DONE: &str = "dlna:discovery:done";
/// Disk format finished (published by the NAS format-disk mutation).
pub const EVENT_DISK_FORMAT_DONE: &str = "disk:format:done";

type PlainHandler = Arc<dyn Fn(JsonValue) + Send + Sync + 'static>;
type CidHandler = Arc<dyn Fn(String, JsonValue) + Send + Sync + 'static>;

static BUS: Lazy<Bus> = Lazy::new(Bus::default);

pub struct Bus {
    plain: Mutex<HashMap<u64, (String, PlainHandler)>>,
    cid: Mutex<HashMap<u64, (String, CidHandler)>>,
    counter: AtomicU64,
}

impl Default for Bus {
    fn default() -> Self {
        Self {
            plain: Mutex::new(HashMap::new()),
            cid: Mutex::new(HashMap::new()),
            counter: AtomicU64::new(0),
        }
    }
}

impl Bus {
    pub fn new() -> &'static Bus {
        &BUS
    }

    /// Broadcast subscription. Handlers receive every event published
    /// under `name` regardless of the cid (or lack thereof) the
    /// publisher passed. Mirrors Go's `Subscribe(topic, fn any)` for
    /// 1-arg handler signatures.
    pub fn subscribe<F: Fn(JsonValue) + Send + Sync + 'static>(
        &self,
        name: &str,
        handler: F,
    ) -> u64 {
        let id = self.counter.fetch_add(1, Ordering::Relaxed);
        self.plain
            .lock()
            .insert(id, (name.to_string(), Arc::new(handler)));
        id
    }
    /// Per-client subscription. Handlers receive `(event_cid, payload)`
    /// and should filter on `event_cid` themselves if they only care
    /// about a specific client. Mirrors Go's `Subscribe(topic, fn any)`
    /// for 2-arg handler signatures.
    pub fn subscribe_with_cid<F: Fn(String, JsonValue) + Send + Sync + 'static>(
        &self,
        name: &str,
        handler: F,
    ) -> u64 {
        let id = self.counter.fetch_add(1, Ordering::Relaxed);
        self.cid
            .lock()
            .insert(id, (name.to_string(), Arc::new(handler)));
        id
    }
    pub fn unsubscribe(&self, id: u64) {
        self.plain.lock().remove(&id);
        self.cid.lock().remove(&id);
    }
    /// Broadcast publish (no cid). Mirrors Go's `Publish(topic, payload)`
    /// 1-arg form.
    pub fn publish(&self, name: &str, payload: JsonValue) {
        let snapshot: Vec<PlainHandler> = {
            let g = self.plain.lock();
            g.values()
                .filter(|(n, _)| n == name)
                .map(|(_, h)| Arc::clone(h))
                .collect()
        };
        for h in snapshot {
            h(payload.clone());
        }
    }
    /// Per-client publish. Mirrors Go's `Publish(topic, cid, payload)`
    /// 2-arg form.
    pub fn publish_with_cid(&self, name: &str, cid: &str, payload: JsonValue) {
        let snapshot: Vec<CidHandler> = {
            let g = self.cid.lock();
            g.values()
                .filter(|(n, _)| n == name)
                .map(|(_, h)| Arc::clone(h))
                .collect()
        };
        for h in snapshot {
            h(cid.to_string(), payload.clone());
        }
    }
}

pub struct EventBus;
impl EventBus {
    pub fn global() -> &'static Bus {
        Bus::new()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/media/eventbus.rs"]
mod tests;
