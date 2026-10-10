//! Unit tests for `src/db/mod.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

// Nanos-unique temp dir so repeated `cargo test` runs never reopen a
// database left behind by an earlier run.
fn tmp_dir(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("plain-nas-{tag}-{nanos}"));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn round_trip_and_prefix_scan() {
    let dir = tmp_dir("db-roundtrip");
    let db = Db::open(&dir).unwrap();
    db.insert("tag:a", "1").unwrap();
    db.insert("tag:b", "2").unwrap();
    db.insert("other:c", "3").unwrap();
    let mut seen = Vec::new();
    for kv in db.scan_prefix("tag:") {
        let (k, v) = kv.unwrap();
        seen.push((
            String::from_utf8_lossy(&k).to_string(),
            String::from_utf8_lossy(&v).to_string(),
        ));
    }
    assert_eq!(
        seen,
        vec![
            ("tag:a".to_string(), "1".to_string()),
            ("tag:b".to_string(), "2".to_string())
        ]
    );
    db.remove("tag:a").unwrap();
    assert_eq!(db.get("tag:a").unwrap(), None);
}

#[test]
fn batch_is_atomic_staging_area() {
    let dir = tmp_dir("db-batch");
    let db = Db::open(&dir).unwrap();
    db.insert("k1", "v1").unwrap();
    let mut batch = db.batch();
    batch.insert("k2", "v2");
    batch.remove("k1");
    db.apply_batch(batch).unwrap();
    assert_eq!(db.get("k1").unwrap(), None);
    assert_eq!(db.get("k2").unwrap().as_deref(), Some(&b"v2"[..]));
}
