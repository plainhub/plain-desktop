//! Tests for `src/media/paths.rs` — the process-global media path source.
use crate::media::paths;

#[test]
fn detect_reads_env_fallback() {
    // The env fallback path: no override installed in the test binary
    // unless another test pinned it; either way detect() must return a
    // usable pair (pin installs PLAIN_RS_DATA_DIR under target/).
    let p = paths::detect();
    assert!(p.data_dir.components().count() > 0);
    assert!(p.cache_dir.components().count() > 0);
}

#[test]
fn pin_test_data_dir_is_stable() {
    let a = paths::pin_test_data_dir();
    let b = paths::pin_test_data_dir();
    assert_eq!(a, b, "the pin must be one fixed directory per test binary");
    assert!(a.ends_with("target/test-media-data-dir"));
    assert!(a.is_dir());
}
