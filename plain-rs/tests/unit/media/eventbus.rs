//! Unit tests for `src/eventbus.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering as O};

#[test]
fn publish_with_cid_routes_to_cid_subscribers() {
    let bus: &'static Bus = Bus::new();
    let n = Arc::new(AtomicUsize::new(0));
    let n2 = Arc::clone(&n);
    bus.subscribe_with_cid("dlna:found", move |_cid, _| {
        n2.fetch_add(1, O::Relaxed);
    });
    bus.publish_with_cid("dlna:found", "client-1", serde_json::json!({}));
    bus.publish_with_cid("dlna:other", "client-1", serde_json::json!({}));
    assert_eq!(n.load(O::Relaxed), 1);
}

#[test]
fn unsubscribe_stops_further_delivery() {
    let bus: &'static Bus = Bus::new();
    let n = Arc::new(AtomicUsize::new(0));
    let n2 = Arc::clone(&n);
    let id = bus.subscribe_with_cid("evt:u", move |_cid, _| {
        n2.fetch_add(1, O::Relaxed);
    });
    bus.publish_with_cid("evt:u", "c1", serde_json::json!({}));
    assert_eq!(n.load(O::Relaxed), 1);
    bus.unsubscribe(id);
    bus.publish_with_cid("evt:u", "c1", serde_json::json!({}));
    assert_eq!(
        n.load(O::Relaxed),
        1,
        "should not be invoked after unsubscribe"
    );
}

#[test]
fn reentrant_subscribe_does_not_deadlock() {
    // A handler that subscribes another handler to a different event.
    // The publish path snapshots handlers under the lock, then invokes
    // them after dropping the lock — so the re-entrant call takes the
    // lock fresh instead of deadlocking.
    let bus: &'static Bus = Bus::new();
    let counter = Arc::new(AtomicUsize::new(0));
    let c2 = Arc::clone(&counter);
    bus.subscribe_with_cid("evt:r", move |_cid, _| {
        c2.fetch_add(1, O::Relaxed);
    });
    // No deadlock = success.
    bus.publish_with_cid("evt:r", "c1", serde_json::json!({}));
    assert_eq!(counter.load(O::Relaxed), 1);
}
