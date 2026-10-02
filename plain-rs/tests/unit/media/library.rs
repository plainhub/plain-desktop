//! Unit tests for `src/library.rs` — the NAS media-index seam over the
//! shared plain-rs library core (track hydration + library-source
//! resolution). The cross-platform behavior locks live in plain-rs.
use super::*;
use crate::media_scan::MediaFile;
use std::sync::Arc;

fn test_db(tag: &str) -> crate::db::Db {
    let p = std::env::temp_dir().join(format!("plain-nas-lib-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    crate::db::Db::open(&p.join("fjall")).unwrap()
}

fn tmp_library(tag: &str) -> Db {
    let p = std::env::temp_dir().join(format!("plain-nas-libdb-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    Db::open(&p.join("plain.db")).unwrap()
}

fn tmp_index() -> Arc<MediaSearchIndex> {
    let dir = tempfile::tempdir().unwrap();
    let idx = MediaSearchIndex::open(dir.path()).unwrap();
    std::mem::forget(dir);
    Arc::new(idx)
}

fn mf_audio(path: &str, kind: &str, title: &str, duration: u32, modified: i64) -> MediaFile {
    use std::hash::{Hash, Hasher};
    let name = std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut h);
    MediaFile {
        uuid: format!("{:016x}", h.finish()),
        fsuuid: String::new(),
        ino: 0,
        ctime: 0,
        duration_sec: duration,
        duration_ref_mod: 0,
        duration_ref_size: 0,
        artist: "Artist".to_string(),
        artist_ref_mod: 0,
        artist_ref_size: 0,
        title: title.to_string(),
        title_ref_mod: 0,
        title_ref_size: 0,
        path: path.to_string(),
        original_path: path.to_string(),
        name,
        size: 1,
        modified_at: modified,
        r#type: kind.to_string(),
        is_trash: false,
        trash_path: String::new(),
        deleted_at: 0,
    }
}

#[test]
fn playlist_audio_from_path_uses_index_row_and_file_stem_fallback() {
    let db = test_db("from_path");
    let mf = mf_audio("/tqC/song.mp3", "audio", "Indexed Title", 42, 1);
    crate::media_scan::upsert_media_row(&db, &mf).unwrap();
    let a = playlist_audio_from_path(&db, "/tqC/song.mp3");
    assert_eq!(a.title, "Indexed Title");
    assert_eq!(a.artist, "Artist");
    assert_eq!(a.duration_ms, 42_000);

    // Unindexed, nonexistent file: title falls back to the file stem,
    // tags/duration are empty/zero — never an error.
    let b = playlist_audio_from_path(&db, "/tqC/unknown_song.flac");
    assert_eq!(b.title, "unknown_song");
    assert_eq!(b.artist, "");
    assert_eq!(b.duration_ms, 0);
}

#[test]
fn empty_title_in_index_row_falls_back_to_file_stem() {
    let db = test_db("stem_fallback");
    let mf = mf_audio("/tqD/plain.mp3", "audio", "", 7, 1);
    crate::media_scan::upsert_media_row(&db, &mf).unwrap();
    let a = playlist_audio_from_path(&db, "/tqD/plain.mp3");
    assert_eq!(a.title, "plain");
    assert_eq!(a.duration_ms, 7_000);
}

#[test]
fn library_source_hydrates_missing_metadata() {
    let db = Arc::new(test_db("hydrate"));
    let index = tmp_index();
    // Row pointing at a real tagged fixture but with no probed metadata —
    // exactly what the scanner leaves behind.
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../plain-rs/testdata/audio-tagged.mp3"
    );
    let mut mf = mf_audio(path, "audio", "", 0, 42);
    mf.artist.clear();
    index.add_media_file(&mf).unwrap();
    crate::media_scan::upsert_media_row(&db, &mf).unwrap();
    index.commit().unwrap();

    let library = tmp_library("hydrate");
    let mut tracks = NasLibraryTracks::new(db, index);

    // playAllAudios: the start track must come back with probed tags and
    // duration, not zeros.
    let start = plain_rs::library::audio_queue::set_library_source(
        &library,
        &mut tracks,
        None,
        false,
        "DATE_DESC",
    )
    .unwrap()
    .unwrap();
    assert!(start.duration_ms > 0);
    assert_eq!(start.artist, "Hydrate Artist");
    assert_eq!(start.title, "Hydrate Title");

    // The queue page (library segment) serves the hydrated values too.
    let page =
        plain_rs::library::audio_queue::queue_page(&library, &mut tracks, 0, 10, "").unwrap();
    assert_eq!(page.len(), 1);
    assert!(page[0].duration_ms > 0);
    assert_eq!(page[0].artist, "Hydrate Artist");
}

#[test]
fn library_locate_and_contains_point_lookups() {
    let db = Arc::new(test_db("locate"));
    let index = tmp_index();
    for (path, modified) in [("/lb/t1.mp3", 100i64), ("/lb/t2.mp3", 200)] {
        let mf = mf_audio(path, "audio", path, 10, modified);
        index.add_media_file(&mf).unwrap();
        crate::media_scan::upsert_media_row(&db, &mf).unwrap();
    }
    index.commit().unwrap();
    let mut tracks = NasLibraryTracks::new(db, index);
    assert_eq!(tracks.library_count().unwrap(), 2);
    assert!(tracks.library_contains("/lb/t1.mp3").unwrap());
    assert!(!tracks.library_contains("/lb/missing.mp3").unwrap());
    let located = tracks.library_locate("/lb/t2.mp3", "DATE_DESC").unwrap();
    assert!((0..2).contains(&located));
    assert_eq!(
        tracks
            .library_locate("/lb/missing.mp3", "DATE_DESC")
            .unwrap(),
        -1
    );
    // Path identity without probing metadata.
    let p = tracks.library_path_at(0, "DATE_DESC").unwrap().unwrap();
    assert!(p == "/lb/t1.mp3" || p == "/lb/t2.mp3");
}
