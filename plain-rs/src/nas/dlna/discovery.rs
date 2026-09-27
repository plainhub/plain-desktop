//! Long-lived renderer discovery task + in-memory cache + event
//! publishing.
//!
//! Mirrors `internal/dlna/discovery_async.go`. We use `tokio::spawn` to
//! run a single discovery loop (one per process) that:
//!
//! 1. Runs SSDP scans in a 60s window with 10s rounds and 1.2s pauses.
//! 2. Caches every successfully-described device by UDN.
//! 3. Publishes `dlna:renderer:found` to the per-client bus for every
//!    newly-discovered device.
//! 4. On window close, publishes `dlna:discovery:done` to every
//!    registered client and clears the client registry (next call will
//!    spawn a new task).
//!
//! Concurrency model:
//!   * One mutex on the manager (`running` flag + `clients` set).
//!   * One RwLock on the cache (multiple readers for the GraphQL
//!     resolver, one writer from the discovery task).
//!
//! Event publishing uses the global `eventbus::Bus`, which the WS hub
//! already subscribes to (`subscribe_with_cid`).

use std::sync::Mutex;
use std::time::Duration;

use parking_lot::RwLock;
use std::sync::LazyLock as Lazy;

use super::desc;
use super::types::{DiscoveredDevice, Renderer};
use crate::media::eventbus;
use crate::nas::consts;

/// Total length of one discovery window. Matches Go's `60s` outer cap.
const DISCOVERY_WINDOW: Duration = Duration::from_secs(60);
/// Pause between rounds inside the window. Go: 1.2s.
const DISCOVERY_INTER_ROUND: Duration = Duration::from_millis(1200);

struct Manager {
    inner: Mutex<ManagerInner>,
}

struct ManagerInner {
    running: bool,
    clients: std::collections::HashSet<String>,
}

impl Manager {
    fn new() -> Self {
        Self {
            inner: Mutex::new(ManagerInner {
                running: false,
                clients: std::collections::HashSet::new(),
            }),
        }
    }
}

static MANAGER: Lazy<Manager> = Lazy::new(Manager::new);

static CACHE: Lazy<RwLock<std::collections::HashMap<String, DiscoveredDevice>>> =
    Lazy::new(|| RwLock::new(std::collections::HashMap::new()));

/// Tests serialise on this lock so the shared static CACHE doesn't get
/// polluted across `cargo test`'s parallel test runs.
#[cfg(test)]
static CACHE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Join the discovery task. Idempotent: if a task is already running,
/// this only registers `client_id` and flushes the current cache. Mirrors
/// Go `StartRendererDiscovery(clientID)`.
pub fn start_renderer_discovery(client_id: &str) {
    let cid = client_id.trim();
    if cid.is_empty() {
        return;
    }

    // Register client and (if not already running) spawn a task.
    let already_running = {
        let mut g = MANAGER.inner.lock().expect("dlna mgr lock poisoned");
        g.clients.insert(cid.to_string());
        let was_running = g.running;
        if !g.running {
            g.running = true;
        }
        was_running
    };
    flush_cache_to_client(cid);

    if already_running {
        return;
    }

    // Spawn the long-lived task. We use a blocking thread because SSDP is
    // a synchronous UDP loop (it doesn't await on tokio's reactor except
    // for the `set_read_timeout` we drive in a poll loop). This keeps
    // the code 1:1 with the Go version.
    let _ = std::thread::Builder::new()
        .name("plain-nas-dlna-discovery".into())
        .spawn(run_discovery_task);
}

fn run_discovery_task() {
    let started = std::time::Instant::now();
    while started.elapsed() < DISCOVERY_WINDOW {
        let _ = desc::discover_upnp_devices(&["ssdp:all".to_string()], Some(on_device));
        std::thread::sleep(DISCOVERY_INTER_ROUND);
    }
    // Window closed: publish `done` to every registered client and
    // reset the manager so the next `start_renderer_discovery` spawns
    // a fresh task.
    let clients: Vec<String> = {
        let mut g = MANAGER.inner.lock().expect("dlna mgr lock poisoned");
        g.running = false;
        let v: Vec<String> = g.clients.drain().collect();
        v
    };
    let payload = super::discovery_done_payload();
    for cid in clients {
        eventbus::EventBus::global().publish_with_cid(
            consts::EVENT_DLNA_DISCOVERY_DONE,
            &cid,
            payload.clone(),
        );
    }
}

fn on_device(d: &DiscoveredDevice) {
    if !d.has_av_transport || d.udn.trim().is_empty() {
        return;
    }
    let udn = d.udn.trim().to_string();
    let existed = {
        let mut c = CACHE.write();
        let existed = c.contains_key(&udn);
        c.insert(udn.clone(), d.clone());
        existed
    };
    if existed {
        return;
    }

    // Publish to all registered clients.
    let clients: Vec<String> = {
        let g = MANAGER.inner.lock().expect("dlna mgr lock poisoned");
        g.clients.iter().cloned().collect()
    };
    let payload = super::renderer_payload(d);
    for cid in clients {
        eventbus::EventBus::global().publish_with_cid(
            consts::EVENT_DLNA_RENDERER_FOUND,
            &cid,
            payload.clone(),
        );
    }
}

fn flush_cache_to_client(cid: &str) {
    let c = CACHE.read();
    for d in c.values() {
        if !d.has_av_transport || d.udn.is_empty() {
            continue;
        }
        let payload = super::renderer_payload(d);
        eventbus::EventBus::global().publish_with_cid(
            consts::EVENT_DLNA_RENDERER_FOUND,
            cid,
            payload,
        );
    }
}

/// Snapshot the cache sorted by name. Mirrors Go `CachedRenderers()`.
pub fn cached_renderers() -> Vec<Renderer> {
    let c = CACHE.read();
    let mut out: Vec<Renderer> = c
        .values()
        .filter(|d| d.has_av_transport)
        .filter_map(DiscoveredDevice::to_renderer)
        .collect();
    out.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    });
    out
}

/// Look up a cached device by UDN. Used by `desc::find_upnp_device_by_udn`.
pub fn get_cached_by_udn(udn: &str) -> Option<DiscoveredDevice> {
    let c = CACHE.read();
    c.get(udn).cloned()
}

/// Insert a device into the cache (used as a side effect of fresh
/// discovery so subsequent lookups skip the network round-trip).
pub fn put_cache(d: DiscoveredDevice) {
    if d.udn.trim().is_empty() {
        return;
    }
    CACHE.write().insert(d.udn.clone(), d);
}

#[cfg(test)]
#[path = "../../../tests/unit/nas/dlna/discovery.rs"]
mod tests;
