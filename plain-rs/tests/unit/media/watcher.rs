//! Unit tests for `src/watcher.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

fn tmp_db() -> crate::media::kv::Db {
    let dir = tempfile::tempdir().unwrap();
    let db = crate::media::kv::Db::open(dir.path()).unwrap();
    std::mem::forget(dir);
    db
}

fn media_file(uuid: &str, path: &str, kind: &str) -> crate::media::scan::MediaFile {
    crate::media::scan::MediaFile {
        uuid: uuid.to_string(),
        fsuuid: String::new(),
        ino: 0,
        ctime: 0,
        duration_sec: 0,
        duration_ref_mod: 0,
        duration_ref_size: 0,
        artist: String::new(),
        artist_ref_mod: 0,
        artist_ref_size: 0,
        title: String::new(),
        title_ref_mod: 0,
        title_ref_size: 0,
        path: path.to_string(),
        original_path: path.to_string(),
        name: std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string(),
        size: 1,
        modified_at: 1,
        r#type: kind.to_string(),
        is_trash: false,
        trash_path: String::new(),
        deleted_at: 0,
    }
}

fn put_media_row(db: &crate::media::kv::Db, m: &crate::media::scan::MediaFile) {
    let mut batch = db.batch();
    batch.insert(
        format!("media:uuid:{}", m.uuid),
        serde_json::to_vec(m).unwrap(),
    );
    db.apply_batch(batch).unwrap();
}

fn tmp_index() -> crate::media::image_index::MediaSearchIndex {
    let dir = tempfile::tempdir().unwrap();
    let idx = crate::media::image_index::MediaSearchIndex::open(dir.path()).unwrap();
    std::mem::forget(dir);
    idx
}

#[test]
fn heal_rebuilds_empty_index_from_kv_rows() {
    let db = tmp_db();
    put_media_row(&db, &media_file("u1", "/x/a.png", "image"));
    put_media_row(&db, &media_file("u2", "/x/b.mp4", "video"));
    let idx = tmp_index();
    assert_eq!(idx.doc_count(), 0);

    let n = heal_media_index_at(&idx, &db);
    assert_eq!(n, 2, "both KV rows must be indexed");
    assert_eq!(idx.doc_count(), 2);
    assert_eq!(
        idx.count("", Some("image"), None).unwrap(),
        1,
        "rebuilt index must answer type-filtered counts"
    );
}

#[test]
fn heal_skips_populated_index() {
    let db = tmp_db();
    put_media_row(&db, &media_file("u1", "/x/a.png", "image"));
    let idx = tmp_index();
    // A stale doc that exists only in the index: a rebuild would wipe it.
    idx.index_media_file(&media_file("stale", "/gone/c.jpg", "image"))
        .unwrap();
    assert_eq!(idx.doc_count(), 1);

    assert_eq!(heal_media_index_at(&idx, &db), 0);
    assert_eq!(idx.doc_count(), 1, "non-empty index must be left alone");
}

#[test]
fn heal_skips_when_kv_has_no_rows() {
    let db = tmp_db();
    let idx = tmp_index();
    assert_eq!(heal_media_index_at(&idx, &db), 0);
    assert_eq!(idx.doc_count(), 0);
}
