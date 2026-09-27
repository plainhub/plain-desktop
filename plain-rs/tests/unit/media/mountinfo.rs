//! Unit tests for `src/mountinfo.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

#[test]
#[cfg(target_os = "linux")]
fn resolve_root() {
    // Almost any path resolves to some mountpoint.
    let mp = resolve_mount_point("/").unwrap();
    assert!(!mp.is_empty());
}
