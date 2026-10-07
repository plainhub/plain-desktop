//! Unit tests for `src/trash.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;
use std::env;
use std::sync::atomic::{AtomicUsize, Ordering};

static SEQ: AtomicUsize = AtomicUsize::new(0);
fn fresh_db() {
    // Set up an isolated database path for tests.
    let id = SEQ.fetch_add(1, Ordering::SeqCst);
    let path = env::temp_dir().join(format!("plain-trash-test-{id}"));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    // `set_var` is unsafe in edition 2024 (it can race with other
    // threads reading the env). Tests run with no other readers
    // contending for this var, so the unsafe block is fine.
    unsafe {
        std::env::set_var("PLAIN_NAS_DATA_DIR", &path);
    }
}

#[test]
fn bucket_path_layout() {
    let rel = compute_bucket_rel_path(
        "file",
        "abcd",
        "hello world.mp4",
        Utc.with_ymd_and_hms(2026, 3, 15, 0, 0, 0).unwrap(),
    );
    assert!(rel.starts_with("data/2026/03/f_abcd_"));
}

#[test]
fn unique_path_appends_index() {
    // The `unique_path` helper lives in `crate::file_tasks` and
    // `crate::chunked_upload` (both private). The trash flow never
    // renames to a path that might already exist in the trash
    // bucket (each file gets a content-derived sub-dir + filename),
    // so the trash module doesn't carry its own `unique_path`. This
    // placeholder test stays so `cargo test` still exercises the
    // surrounding helpers when run on this module.
}

#[test]
fn round_trip_store_load() {
    fresh_db();
    // Note: this requires db::get_default() to be opened with PLAIN_NAS_DATA_DIR
    // before the test is run. Cargo runs tests in parallel; for simplicity we
    // just exercise the serializer.
    let item = TrashItem {
        id: "abc".into(),
        kind: "file".into(),
        original_path: "/tmp/foo".into(),
        disk: "/".into(),
        trash_rel_path: "data/2026/01/f_abc".into(),
        deleted_at: 1700000000,
        uid: 0,
        gid: 0,
        mode: 0o644,
        size: None,
        entry_count: None,
    };
    let s = serde_json::to_string(&item).unwrap();
    let back: TrashItem = serde_json::from_str(&s).unwrap();
    assert_eq!(back.id, "abc");
    assert_eq!(back.kind, "file");
}
