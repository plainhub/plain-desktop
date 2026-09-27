//! Unit tests for `src/temp_store.rs` — compiled as the `tests` child
//! module via `#[cfg(test)] #[path]` there.
use super::*;

#[test]
fn take_returns_value_once() {
    let key = format!("k-{}", std::process::id());
    assert_eq!(take(&key), None);
    set(&key, "v");
    assert_eq!(take(&key), Some("v".into()));
    assert_eq!(take(&key), None, "second take: consumed");
    assert_eq!(get(&key), None);
}

#[test]
fn set_overwrites() {
    let key = format!("k2-{}", std::process::id());
    set(&key, "a");
    set(&key, "b");
    assert_eq!(get(&key), Some("b".into()));
    take(&key);
}
