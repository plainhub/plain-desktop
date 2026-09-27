//! Unit tests for `src/media/thumb_engine/singleflight.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

#[tokio::test]
async fn same_key_serializes_with_mutual_exclusion() {
    let locks = Arc::new(KeyedLocks::new());
    let counter = Arc::new(AtomicUsize::new(0));
    let max_seen = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();
    for _ in 0..32 {
        let locks_ref = locks.clone();
        let counter = counter.clone();
        let max_seen = max_seen.clone();
        handles.push(tokio::spawn(async move {
            locks_ref
                .with_lock("k".to_string(), async move {
                    let now = counter.fetch_add(1, Ordering::SeqCst) + 1;
                    max_seen.fetch_max(now, Ordering::SeqCst);
                    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
                    counter.fetch_sub(1, Ordering::SeqCst);
                })
                .await
        }));
    }
    for h in handles {
        h.await.unwrap();
    }
    assert_eq!(
        max_seen.load(Ordering::SeqCst),
        1,
        "critical section overlapped"
    );
}

#[tokio::test]
async fn different_keys_overlap() {
    let locks = Arc::new(KeyedLocks::new());
    let in_a = Arc::new(AtomicUsize::new(0));
    let ia2 = in_a.clone();
    let locks_a = locks.clone();
    let a = tokio::spawn(async move {
        locks_a
            .with_lock("a".into(), async move {
                ia2.store(1, Ordering::SeqCst);
                tokio::time::sleep(std::time::Duration::from_millis(80)).await
            })
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    let entered = in_a.load(Ordering::SeqCst) == 1;
    // "b" must not wait for the sleeping "a" holder.
    let b_start = std::time::Instant::now();
    locks.clone().with_lock("b".into(), async {}).await;
    assert!(b_start.elapsed() < std::time::Duration::from_millis(50));
    a.await.unwrap();
    assert!(entered);
}

#[tokio::test]
async fn map_does_not_leak() {
    let locks = KeyedLocks::new();
    for _ in 0..100 {
        locks.with_lock("ephemeral".into(), async {}).await;
    }
    assert_eq!(locks.in_flight_keys(), 0);
}
