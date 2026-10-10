//! Unit tests for `src/db/media_source_dirs.rs` — compiled as the `tests`
//! child module via `#[cfg(test)] #[path]` there.
use super::*;

#[test]
fn round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Prefs::load(&dir.path().join("prefs.json")).unwrap();
    assert!(get(&prefs).is_empty());
    set(&prefs, &["/mnt/a".to_string(), "/mnt/b".to_string()]).unwrap();
    assert_eq!(get(&prefs), vec!["/mnt/a", "/mnt/b"]);
}
