//! Unit tests for `src/media/cover.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

#[test]
fn extract_cover_nonexistent() {
    assert!(extract_cover("/nonexistent/file.mp3").is_none());
}

#[test]
fn thumbnail_cache_ref_path_no_sidecar() {
    let p = "/music/song.mp3";
    assert_eq!(thumbnail_cache_ref_path(p), p);
}

#[test]
fn find_sidecar_returns_none_for_missing() {
    assert!(find_sidecar_cover_path("/nonexistent/song.mp3").is_none());
}
