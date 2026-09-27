//! Unit tests for `src/file_tasks.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;
use std::env;

static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
fn fresh_db() {
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let p = env::temp_dir().join(format!("plain-nas-tasks-{n}"));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    let _ = crate::media::kv::open(&p.join("fjall")).unwrap();
}

#[test]
fn compute_totals_single_file() {
    let dir = env::temp_dir().join("plain-nas-totals-file");
    let _ = std::fs::create_dir_all(&dir);
    let f = dir.join("a.bin");
    std::fs::write(&f, b"12345").unwrap();
    let (b, i) = compute_totals(&[FileTaskOp {
        src: f.to_string_lossy().to_string(),
        dst: dir.join("b.bin").to_string_lossy().to_string(),
        overwrite: false,
    }]);
    assert_eq!(b, 5);
    assert_eq!(i, 1);
    let _ = std::fs::remove_file(&f);
}

#[test]
fn compute_totals_dir() {
    let dir = env::temp_dir().join("plain-nas-totals-dir");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("a.txt"), b"hi").unwrap();
    std::fs::write(dir.join("sub/b.txt"), b"world!").unwrap();
    let (b, i) = compute_totals(&[FileTaskOp {
        src: dir.to_string_lossy().to_string(),
        dst: dir.with_extension("_out").to_string_lossy().to_string(),
        overwrite: false,
    }]);
    assert_eq!(b, 2 + 6);
    assert_eq!(i, 2);
}

#[tokio::test]
async fn round_trip_persist() {
    fresh_db();
    let t = create_copy_task(
        "cid",
        vec![FileTaskOp {
            src: "/a".into(),
            dst: "/b".into(),
            overwrite: false,
        }],
    )
    .unwrap();
    assert_eq!(t.client_id, "cid");
    assert_eq!(t.status, FileTaskStatus::Queued);
    // Give the worker a moment to record at least the initial snapshot.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let tasks = list_tasks("cid").unwrap();
    assert!(!tasks.is_empty());
}
