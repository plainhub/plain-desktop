//! Unit tests for `src/media/thumb_engine/lru.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

fn key(n: u32) -> PathBuf {
    PathBuf::from(format!("/cache/thumbs/ab/{n:04}.jpg"))
}

fn data(n: usize) -> Arc<Vec<u8>> {
    Arc::new(vec![7u8; n])
}

#[test]
fn put_get_and_byte_cap_eviction() {
    let lru = ThumbLru::new(256, 100); // 256 bytes
    lru.put(key(1), data(100));
    lru.put(key(2), data(100));
    lru.put(key(3), data(100)); // 300 > 256 → oldest (1) evicted
    assert!(lru.get(&key(1)).is_none());
    assert!(lru.get(&key(2)).is_some());
    assert!(lru.get(&key(3)).is_some());
    let (n, bytes) = lru.stats();
    assert_eq!(n, 2);
    assert_eq!(bytes, 200);
}

#[test]
fn touch_changes_eviction_order() {
    let lru = ThumbLru::new(256, 100);
    lru.put(key(1), data(100));
    lru.put(key(2), data(100));
    // Touch 1 so 2 becomes the LRU victim.
    std::thread::sleep(std::time::Duration::from_millis(2));
    let _ = lru.get(&key(1));
    lru.put(key(3), data(100));
    assert!(
        lru.get(&key(2)).is_none(),
        "untouched entry must be evicted"
    );
    assert!(lru.get(&key(1)).is_some());
}

#[test]
fn entry_count_cap() {
    let lru = ThumbLru::new(1 << 20, 3);
    for i in 0..5 {
        lru.put(key(i), data(10));
    }
    let (n, _) = lru.stats();
    assert!(n <= 3, "entries {n} exceed cap");
    assert!(lru.get(&key(0)).is_none(), "oldest evicted");
    assert!(lru.get(&key(4)).is_some(), "newest kept");
}

#[test]
fn replace_same_key_does_not_double_count() {
    let lru = ThumbLru::new(1 << 20, 100);
    lru.put(key(1), data(100));
    lru.put(key(1), data(200));
    let (n, bytes) = lru.stats();
    assert_eq!((n, bytes), (1, 200));
}
