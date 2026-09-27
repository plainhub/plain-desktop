//! Unit tests for `src/dlna/media_alias.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

#[test]
fn register_stores_alias() {
    // The registry is global; this test only asserts that the
    // returned id round-trips through `lookup`.
    let (id, ext) = register("/a/MOVIE.MP4", "video/mp4");
    assert_eq!(ext, "mp4");
    let looked = lookup(&id);
    assert!(looked.is_some(), "alias should be stored");
    let (path, mime) = looked.unwrap();
    assert_eq!(path, "/a/MOVIE.MP4");
    assert_eq!(mime, "video/mp4");
}

#[test]
fn lookup_unknown_returns_none() {
    assert!(lookup("does-not-exist-xyz").is_none());
    assert!(lookup("").is_none());
    assert!(lookup("   ").is_none());
}

#[test]
fn register_rejects_unsafe_extension() {
    // `mp4?foo=1` is not `[a-z0-9]{1,16}` → falls back to "bin".
    let (_id, ext) = register("/a/x.mp4?foo=1", "video/mp4");
    assert_eq!(ext, "bin");
}

#[test]
fn register_no_extension_yields_bin() {
    let (_id, ext) = register("/a/MOVIE", "video/mp4");
    assert_eq!(ext, "bin");
}

#[test]
fn safe_media_url_passes_through_non_fs() {
    let url = "http://other.example/path?x=1";
    assert_eq!(safe_media_url(url, "video/mp4"), url);
}

#[test]
fn safe_media_url_passes_through_missing_id() {
    let url = "http://other.example/fs?other=1";
    assert_eq!(safe_media_url(url, "video/mp4"), url);
}

#[test]
fn safe_media_url_passes_through_empty_id() {
    let url = "http://other.example/fs?id=";
    assert_eq!(safe_media_url(url, "video/mp4"), url);
}

#[test]
fn base36_known_values() {
    assert_eq!(base36(0), "0");
    assert_eq!(base36(35), "z");
    assert_eq!(base36(36), "10");
    // 1_000_000 in base 36 is 21*36^3 + 15*36^2 + 33*36 + 28 = "lfls".
    assert_eq!(base36(1_000_000), "lfls");
}

#[test]
fn is_safe_ext_rejects_path_traversal() {
    assert!(!is_safe_ext("../etc"));
    assert!(!is_safe_ext("a/b"));
    assert!(!is_safe_ext(""));
    assert!(!is_safe_ext(&"x".repeat(17)));
    assert!(is_safe_ext("mp4"));
}
