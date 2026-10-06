//! Unit tests for `src/media_scan.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

/// Test tree in a non-excluded location: the default tempdir is under
/// `/var/folders/…/.tmpXXX` on macOS, which the media exclusions
/// (rightly) refuse to index. `$HOME` keeps the paths clean.
fn tree() -> tempfile::TempDir {
    let base = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    tempfile::Builder::new()
        .prefix("plainnas-test-")
        .tempdir_in(base)
        .unwrap()
}

fn tmp_db() -> Arc<crate::media::kv::Db> {
    let dir = tree();
    let db = crate::media::kv::Db::open(dir.path()).unwrap();
    crate::test_tempdirs::retain(dir);
    Arc::new(db)
}

#[test]
fn media_exclusions() {
    let data = Path::new("/opt/plainnas/data");
    let cache = Path::new("/opt/plainnas/cache");
    let extra = vec!["/home/u/old".to_string()];
    let excl = |p: &str| is_media_excluded_name_or_root(p, data, cache, &extra);

    // System and virtual roots.
    assert!(excl("/usr/share/icons/a.png"));
    assert!(excl("/var/log/x.log"));
    assert!(excl("/proc/1/status"));
    // Component boundary: /usr2 is NOT /usr.
    assert!(!excl("/usr2/photo.jpg"));
    // User data areas stay in.
    assert!(!excl("/DATA/Gallery/photo.jpg"));
    assert!(!excl("/home/u/Photos/a.jpg"));
    assert!(!excl("/mnt/usb1/dcam/b.jpg"));
    // Hidden entries anywhere (also covers the app's `.nas-trash`).
    assert!(excl("/DATA/.thumbs/a.jpg"));
    assert!(excl("/home/u/proj/.git/HEAD"));
    assert!(excl("/disk/.nas-trash/old.png"));
    assert!(excl("/home/u/web/node_modules/lib.js"));
    assert!(!excl("/home/u/app/target/debug/x"));
    assert!(!excl("/srv/site/dist/icon.svg"));
    assert!(!excl("/srv/site/BUILD/style.css"));
    assert!(excl(
        "/Users/alice/Movies/CapCut/User Data/Cache/effect/7408409772950637830/image/blusher.png"
    ));
    assert!(excl(
        "/Users/alice/Library/Browser/User Data/GPUCache/image.png"
    ));
    assert!(excl(
        "/Users/alice/Library/Browser/User Data/Code Cache/image.png"
    ));
    assert!(!excl("/Users/alice/Pictures/Cache/edited.png"));
    assert!(!excl("/Users/alice/Pictures/Cachet/photo.jpg"));
    // Config-provided extra roots.
    assert!(excl("/home/u/old/legacy.png"));
    assert!(
        !excl("/home/u/oldphoto/x.png"),
        "prefix must respect boundaries"
    );
    // The app's own dirs (thumb cache pollution source).
    assert!(excl("/opt/plainnas/cache/thumbs/ab/x.webp"));
    assert!(excl("/opt/plainnas/data/fjall/000001.sst"));
}

#[test]
fn platform_system_root_matching_respects_path_boundaries_and_windows_case() {
    assert!(under_root_case_insensitive(
        "D:/WINDOWS/System32",
        "D:/Windows"
    ));
    assert!(!under_root_case_insensitive(
        "D:/WindowsOld/file",
        "D:/Windows"
    ));
    #[cfg(target_os = "windows")]
    {
        assert!(is_media_excluded_at(
            "c:/windows/System32/a.dll",
            Path::new("Z:/data"),
            Path::new("Z:/cache"),
            &[]
        ));
        assert!(is_media_excluded_at(
            "D:/Program Files/App/a.exe",
            Path::new("Z:/data"),
            Path::new("Z:/cache"),
            &[]
        ));
    }
    #[cfg(target_os = "macos")]
    {
        assert!(is_media_excluded_at(
            "/System/Library/CoreServices/a",
            Path::new("/tmp/data"),
            Path::new("/tmp/cache"),
            &[]
        ));
        assert!(is_media_excluded_at(
            "/Applications/SomeApp/Resources/icon.png",
            Path::new("/tmp/data"),
            Path::new("/tmp/cache"),
            &[]
        ));
        assert!(!is_media_excluded_at(
            "/Users/alice/Pictures/a.jpg",
            Path::new("/tmp/data"),
            Path::new("/tmp/cache"),
            &[]
        ));
    }
    #[cfg(target_os = "linux")]
    {
        assert!(is_media_excluded_at(
            "/etc/ssl/certs/a.pem",
            Path::new("/tmp/data"),
            Path::new("/tmp/cache"),
            &[]
        ));
        assert!(!is_media_excluded_at(
            "/home/alice/Pictures/a.jpg",
            Path::new("/tmp/data"),
            Path::new("/tmp/cache"),
            &[]
        ));
    }
}

#[test]
fn photos_library_package_is_excluded_from_media_scan_on_macos() {
    #[cfg(target_os = "macos")]
    {
        let data = Path::new("/tmp/data");
        let cache = Path::new("/tmp/cache");
        let excluded = |path: &str| is_media_excluded_at(path, data, cache, &[]);
        assert!(excluded(
            "/Users/alice/Pictures/Photos Library.photoslibrary"
        ));
        assert!(excluded(
            "/Users/alice/Pictures/Photos Library.photoslibrary/resources/derivatives/cvt/photo.jpeg"
        ));
        assert!(excluded(
            "/Volumes/Backup/ALBUM.PHOTOSLIBRARY/originals/photo.jpeg"
        ));
        assert!(!excluded(
            "/Users/alice/Pictures/Photos Library.photoslibrary-backup/photo.jpeg"
        ));
        assert!(!excluded("/Users/alice/Pictures/photo.jpeg"));
    }
}

#[test]
#[cfg(target_os = "macos")]
fn rescan_removes_previously_indexed_photos_library_items_and_buckets() {
    let dir = tree();
    let library = dir.path().join("Photos.photoslibrary");
    let path = library.join("resources/derivatives/cvt/photo.jpg");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"old indexed image").unwrap();

    let db = Arc::new(tmp_db());
    let p = path.to_string_lossy().to_string();
    let row = build_scanned(&db, &p, &std::fs::metadata(&path).unwrap()).unwrap();
    let mut batch = db.batch();
    stage_scanned(&mut batch, &row);
    db.apply_batch(batch).unwrap();
    apply_bucket_deltas(&db, &bucket_deltas_of(std::slice::from_ref(&row)));
    crate::media::image_index::global()
        .index_media_file(&row.m)
        .unwrap();
    assert_eq!(list_buckets(&db, "image").unwrap().len(), 1);

    scan_tree(&db, dir.path(), &Arc::new(Scanner::new()));

    assert!(get_by_path(&db, &p).unwrap().is_none());
    assert!(list_buckets(&db, "image").unwrap().is_empty());
}

#[test]
fn rescan_removes_previously_indexed_application_cache_images() {
    let dir = tree();
    let image = dir
        .path()
        .join("Movies/CapCut/User Data/Cache/effect/image/blusher.png");
    std::fs::create_dir_all(image.parent().unwrap()).unwrap();
    std::fs::write(&image, b"cached image").unwrap();

    let db = Arc::new(tmp_db());
    let path = image.to_string_lossy().to_string();
    assert!(scan_file(&db, &path).is_err());
    let row = build_scanned(&db, &path, &std::fs::metadata(&image).unwrap()).unwrap();
    let mut batch = db.batch();
    stage_scanned(&mut batch, &row);
    db.apply_batch(batch).unwrap();
    apply_bucket_deltas(&db, &bucket_deltas_of(std::slice::from_ref(&row)));
    crate::media::image_index::global()
        .index_media_file(&row.m)
        .unwrap();
    assert_eq!(list_buckets(&db, "image").unwrap().len(), 1);

    scan_tree(&db, dir.path(), &Arc::new(Scanner::new()));

    assert!(get_by_path(&db, &path).unwrap().is_none());
    assert!(list_buckets(&db, "image").unwrap().is_empty());
}

#[test]
fn nomedia_changes_remove_and_restore_subtree_media() {
    let dir = tree();
    let folder = dir.path().join("Pictures/Private");
    std::fs::create_dir_all(&folder).unwrap();
    let photo = folder.join("photo.jpg");
    std::fs::write(&photo, b"photo").unwrap();
    let db = tmp_db();
    let path = photo.to_string_lossy().to_string();

    rescan_subtree(db.clone(), folder.clone()).unwrap();
    assert!(get_by_path(&db, &path).unwrap().is_some());

    std::fs::write(folder.join(".nomedia"), b"").unwrap();
    rescan_subtree(db.clone(), folder.clone()).unwrap();
    assert!(get_by_path(&db, &path).unwrap().is_none());

    std::fs::remove_file(folder.join(".nomedia")).unwrap();
    rescan_subtree(db.clone(), folder).unwrap();
    assert!(get_by_path(&db, &path).unwrap().is_some());
}

#[test]
fn precount_stops_before_rebuild_waits_for_old_scan() {
    let dir = tree();
    std::fs::write(dir.path().join("photo.jpg"), b"photo").unwrap();
    let scanner = Scanner::new();
    scanner.stop();
    assert_eq!(count_files_with_stop(dir.path(), Some(&scanner)), 0);
}

#[test]
fn android_album_art_and_empty_docs_stay_out_of_media_pages() {
    let dir = tree();
    let db = tmp_db();
    for (name, bytes, expected) in [
        ("folder.jpg", b"image".as_slice(), "other"),
        ("AlbumArtSmall.jpg", b"image".as_slice(), "other"),
        ("portrait.jpg", b"image".as_slice(), "image"),
        ("empty.pdf", b"".as_slice(), "other"),
        ("report.pdf", b"document".as_slice(), "doc"),
    ] {
        let path = dir.path().join(name);
        std::fs::write(&path, bytes).unwrap();
        let row = build_scanned(
            &db,
            path.to_str().unwrap(),
            &std::fs::metadata(&path).unwrap(),
        )
        .unwrap();
        assert_eq!(row.m.r#type, expected, "{name}");
    }
}

#[test]
fn scan_tree_skips_excluded_paths() {
    let dir = tree();
    // /…/Pics is the real library; the rest must be skipped.
    std::fs::create_dir_all(dir.path().join("Pics")).unwrap();
    std::fs::create_dir_all(dir.path().join(".thumbs")).unwrap();
    std::fs::create_dir_all(dir.path().join("web/node_modules")).unwrap();
    std::fs::write(dir.path().join("Pics/a.jpg"), b"1").unwrap();
    std::fs::write(dir.path().join(".thumbs/t.webp"), b"2").unwrap();
    std::fs::write(dir.path().join("web/node_modules/lib.js"), b"3").unwrap();
    std::fs::write(dir.path().join(".hidden.jpg"), b"4").unwrap();

    let db = Arc::new(tmp_db());
    let s = Arc::new(Scanner::new());
    let (seen, indexed) = scan_tree(&db, dir.path(), &s);
    assert_eq!(seen, 1);
    assert_eq!(indexed, 1);
    // Precount agrees with the scan (progress can complete).
    assert_eq!(count_files(dir.path()), 1);
    // Only the real image made it into KV and buckets.
    assert_eq!(list_buckets(&db, "image").unwrap().len(), 1);
    assert!(
        get_by_path(&db, dir.path().join(".hidden.jpg").to_str().unwrap())
            .unwrap()
            .is_none(),
        "hidden file must not be indexed"
    );

    // scan_file refuses excluded single paths (watcher re-sends, …).
    let nm = dir.path().join("web/node_modules/new.js");
    std::fs::write(&nm, b"5").unwrap();
    assert!(scan_file(&db, nm.to_str().unwrap()).is_err());
}

#[test]
fn infer_type_known_extensions() {
    assert_eq!(infer_type("a.mp3"), "audio");
    assert_eq!(infer_type("b.MP3"), "audio");
    assert_eq!(infer_type("c.MOV"), "video");
    assert_eq!(infer_type("d.png"), "image");
    assert_eq!(infer_type("e.txt"), "doc");
}

#[test]
fn scan_file_inserts_and_get_by_path() {
    let dir = tree();
    let p = dir.path().join("song.mp3");
    std::fs::write(&p, b"x").unwrap();

    let db = tmp_db();
    let m = scan_file(&db, p.to_str().unwrap()).unwrap();
    assert_eq!(m.r#type, "audio");
    assert_eq!(m.size, 1);
    assert!(!m.uuid.is_empty());

    let by_path = get_by_path(&db, p.to_str().unwrap()).unwrap().unwrap();
    assert_eq!(by_path.uuid, m.uuid);
    assert_eq!(by_path.name, "song.mp3");

    let by_uuid = get_by_uuid(&db, &m.uuid).unwrap().unwrap();
    assert_eq!(by_uuid.path, by_path.path);
}

#[test]
fn scan_file_is_idempotent_by_path() {
    let dir = tree();
    let p = dir.path().join("x.png");
    std::fs::write(&p, b"x").unwrap();

    let db = tmp_db();
    let a = scan_file(&db, p.to_str().unwrap()).unwrap();
    let b = scan_file(&db, p.to_str().unwrap()).unwrap();
    assert_eq!(a.uuid, b.uuid, "same path ⇒ same UUID");
}

#[test]
fn delete_by_uuid_removes_all_index_entries() {
    let dir = tree();
    let p = dir.path().join("k.mp4");
    std::fs::write(&p, b"v").unwrap();

    let db = tmp_db();
    let m = scan_file(&db, p.to_str().unwrap()).unwrap();
    assert!(get_by_uuid(&db, &m.uuid).unwrap().is_some());
    delete_by_uuid(&db, &m.uuid).unwrap();
    assert!(get_by_uuid(&db, &m.uuid).unwrap().is_none());
    assert!(get_by_path(&db, p.to_str().unwrap()).unwrap().is_none());
}

#[test]
fn reset_all_wipes_everything() {
    let dir = tree();
    let p1 = dir.path().join("a.mp3");
    let p2 = dir.path().join("b.png");
    std::fs::write(&p1, b"x").unwrap();
    std::fs::write(&p2, b"y").unwrap();
    let db = tmp_db();
    scan_file(&db, p1.to_str().unwrap()).unwrap();
    scan_file(&db, p2.to_str().unwrap()).unwrap();
    reset_all(&db).unwrap();
    assert!(get_by_path(&db, p1.to_str().unwrap()).unwrap().is_none());
    assert!(get_by_path(&db, p2.to_str().unwrap()).unwrap().is_none());
}

/// The FID secondary index must be wiped together with the primary rows;
/// pre-fix it survived every rebuild and leaked one row per file change.
#[test]
fn reset_all_wipes_fid_index() {
    let dir = tree();
    let p = dir.path().join("a.mp3");
    std::fs::write(&p, b"x").unwrap();
    let db = tmp_db();
    let m = scan_file(&db, p.to_str().unwrap()).unwrap();
    let fid = crate::media::uuid::fid_key(&m.fsuuid, m.ino, m.ctime);
    assert!(db.get(&fid).unwrap().is_some());
    reset_all(&db).unwrap();
    assert!(db.get(&fid).unwrap().is_none());
}

#[test]
fn delete_by_uuid_removes_fid_row() {
    let dir = tree();
    let p = dir.path().join("a.mp3");
    std::fs::write(&p, b"x").unwrap();
    let db = tmp_db();
    let m = scan_file(&db, p.to_str().unwrap()).unwrap();
    let fid = crate::media::uuid::fid_key(&m.fsuuid, m.ino, m.ctime);
    assert!(db.get(&fid).unwrap().is_some());
    delete_by_uuid(&db, &m.uuid).unwrap();
    assert!(db.get(&fid).unwrap().is_none());
}

/// Direct exercise of the parallel buffered engine: nested dirs, rows
/// readable afterwards exactly like per-file writes.
#[test]
fn scan_tree_indexes_nested_tree() {
    let dir = tree();
    std::fs::create_dir_all(dir.path().join("a/b")).unwrap();
    std::fs::write(dir.path().join("a/x.mp3"), b"1").unwrap();
    std::fs::write(dir.path().join("a/b/y.png"), b"2").unwrap();
    std::fs::write(dir.path().join("z.mp4"), b"3").unwrap();
    let db = tmp_db();
    let s = Arc::new(Scanner::new());
    let (seen, indexed) = scan_tree(&db, dir.path(), &s);
    assert_eq!((seen, indexed), (3, 3));
    for rel in ["a/x.mp3", "a/b/y.png", "z.mp4"] {
        let p = dir.path().join(rel);
        let m = get_by_path(&db, p.to_str().unwrap()).unwrap().expect(rel);
        assert!(!m.uuid.is_empty());
    }
}

#[test]
fn scan_tree_removes_missing_media_under_scanned_root() {
    let dir = tree();
    let image = dir.path().join("removed.jpg");
    std::fs::write(&image, b"image").unwrap();
    let db = tmp_db();
    let scanner = Arc::new(Scanner::new());
    assert_eq!(scan_tree(&db, dir.path(), &scanner).0, 1);
    assert!(get_by_path(&db, image.to_str().unwrap()).unwrap().is_some());
    std::fs::remove_file(&image).unwrap();
    assert_eq!(scan_tree(&db, dir.path(), &scanner).0, 0);
    assert!(get_by_path(&db, image.to_str().unwrap()).unwrap().is_none());
}

/// `rebuildMediaIndex` may name a single file; the engine indexes it as
/// one row instead of treating it as an empty directory.
#[test]
fn scan_tree_indexes_single_file_root() {
    let dir = tree();
    let p = dir.path().join("one.mp3");
    std::fs::write(&p, b"1").unwrap();
    let db = tmp_db();
    let s = Arc::new(Scanner::new());
    let (seen, indexed) = scan_tree(&db, &p, &s);
    assert_eq!((seen, indexed), (1, 1));
    assert!(get_by_path(&db, p.to_str().unwrap()).unwrap().is_some());
}

/// Regression guard for the 2026-09 rebuild slowdown: the engine must
/// commit one batch per ~512 files, never one commit per file (which
/// turned rebuilds of large libraries into minutes-long journal appends).
/// Deterministic — counts commits instead of timing, so it holds on any
/// machine regardless of disk speed.
#[test]
fn scan_engine_commits_in_batches_not_per_file() {
    let dir = tree();
    const N: usize = 3_000;
    for i in 0..N {
        std::fs::write(dir.path().join(format!("{i}.mp3")), b"x").unwrap();
    }
    let db = Arc::new(tmp_db());
    let s = Arc::new(Scanner::new());
    let t = std::time::Instant::now();
    let (seen, indexed) = scan_tree(&db, dir.path(), &s);
    assert_eq!((seen, indexed), (N as i64, N as i64));

    // 3000 files ⇒ ≤ 6 worker batches + ≤ 8 worker finals + slack. A
    // per-file-commit regression would produce ~3000 and fail here.
    let commits = s.commits.load(Ordering::SeqCst);
    assert!(
        commits <= 24,
        "scan committed {commits} times for {N} files — per-file commit regression"
    );
    // Catastrophic-slowdown smoke (the original symptom was ~1min stalls):
    // even this small tree must finish promptly. Generous bound so slow
    // machines / cold caches never flake.
    assert!(
        t.elapsed() < Duration::from_secs(30),
        "scan_tree took {:?} for {N} files",
        t.elapsed()
    );
}

#[test]
fn scanner_state_transitions() {
    let s = Scanner::new();
    assert_eq!(s.state(), ScanState::Idle);
    s.resume();
    assert_eq!(s.state(), ScanState::Running);
    s.pause();
    assert_eq!(s.state(), ScanState::Paused);
    s.stop();
    assert_eq!(s.state(), ScanState::Stopped);
}

/// Verify pause_flag and stop_flag are independent (the core bug
/// that caused `resumeMediaScan` to have no progress). Before the
/// fix, `pause()` cleared `stop_flag` and there was no `pause_flag`
/// at all, so the walk loop never actually paused.
#[test]
fn pause_and_stop_flags_are_independent() {
    let s = Scanner::new();
    // Pause should not set stop_flag.
    s.pause();
    assert!(s.is_paused());
    assert!(!s.is_stopping());
    assert_eq!(s.state(), ScanState::Paused);
    // Resume should clear pause_flag but not touch stop_flag.
    s.resume();
    assert!(!s.is_paused());
    assert!(!s.is_stopping());
    assert_eq!(s.state(), ScanState::Running);
    // Stop should set stop_flag; pause after stop should not clear it.
    s.stop();
    assert!(s.is_stopping());
    assert!(!s.is_paused());
    assert_eq!(s.state(), ScanState::Stopped);
    s.pause();
    assert!(s.is_paused());
    assert!(s.is_stopping(), "pause must not clear stop_flag");
}

#[tokio::test]
async fn start_walk_and_scan_indexes_multiple_roots() {
    let dir = tree();
    let first = dir.path().join("first");
    let second = dir.path().join("second");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    std::fs::write(first.join("a.mp3"), b"x").unwrap();
    std::fs::write(first.join("b.png"), b"y").unwrap();
    std::fs::write(second.join("c.mp4"), b"z").unwrap();

    let db = tmp_db();
    start_walk_and_scan_paths(db.clone(), vec![first, second], dir.path().to_path_buf())
        .await
        .unwrap();
    // Wait for the worker to finish (state → idle).
    for _ in 0..200 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if scanner().state() == ScanState::Idle {
            break;
        }
    }
    assert_eq!(scanner().state(), ScanState::Idle);
    let (i, t, _) = scanner().get_progress();
    assert_eq!(i, 3);
    assert_eq!(t, 3);
}

/// Pause semantics of the scan engine, deterministic version: the walk
/// parks immediately when the pause flag is pre-set (no race with scan
/// speed), then completes after resume. Uses a local `Scanner` via
/// `scan_tree` directly — no global singleton involved.
#[test]
fn pause_and_resume_actually_pauses_walk() {
    let dir = tree();
    for i in 0..200 {
        std::fs::write(dir.path().join(format!("{i}.mp3")), b"x").unwrap();
    }

    let db = Arc::new(tmp_db());
    let s = Arc::new(Scanner::new());
    s.pause();
    assert_eq!(s.state(), ScanState::Paused);

    let db2 = db.clone();
    let s2 = s.clone();
    let root = dir.path().to_path_buf();
    let handle = std::thread::spawn(move || scan_tree(&db2, &root, &s2));

    // While paused the walk must make zero progress.
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        s.last_indexed.load(Ordering::SeqCst),
        0,
        "paused walk must not index anything"
    );

    s.resume();
    let (seen, indexed) = handle.join().unwrap();
    assert_eq!(
        (seen, indexed),
        (200, 200),
        "scan must complete after resume"
    );
    assert_eq!(s.last_indexed.load(Ordering::SeqCst), 200);
}

/// Stop breaks the walk promptly; whatever was staged still gets its
/// final commit so the index stays consistent.
#[test]
fn stop_breaks_walk_early() {
    let dir = tree();
    for i in 0..200 {
        std::fs::write(dir.path().join(format!("{i}.mp3")), b"x").unwrap();
    }

    let db = Arc::new(tmp_db());
    let s = Arc::new(Scanner::new());
    // Pre-set stop: every worker bails before processing entries.
    s.stop();

    let db2 = db.clone();
    let s2 = s.clone();
    let root = dir.path().to_path_buf();
    let (seen, indexed) = std::thread::spawn(move || scan_tree(&db2, &root, &s2))
        .join()
        .unwrap();
    assert!(indexed < 200, "stopped walk must not finish the tree");
    assert_eq!(seen, indexed);
}

/// Regression guard for the homepage-zeroes bug: the parallel engine
/// must maintain the per-directory bucket counters the mediaBuckets
/// resolver reads, idempotently (re-scans must not double-count) and
/// deletions must decrement them.
#[test]
fn scan_tree_maintains_bucket_counters() {
    let dir = tree();
    std::fs::create_dir_all(dir.path().join("Pics")).unwrap();
    std::fs::create_dir_all(dir.path().join("Vids")).unwrap();
    std::fs::write(dir.path().join("Pics/a.jpg"), b"1").unwrap();
    std::fs::write(dir.path().join("Pics/b.jpg"), b"2").unwrap();
    std::fs::write(dir.path().join("Vids/c.mp4"), b"3").unwrap();

    let db = Arc::new(tmp_db());
    let s = Arc::new(Scanner::new());
    let (seen, _) = scan_tree(&db, dir.path(), &s);
    assert_eq!(seen, 3);

    let pics = dir.path().join("Pics").to_string_lossy().to_string();
    let vids = dir.path().join("Vids").to_string_lossy().to_string();

    let images = list_buckets(&db, "image").unwrap();
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].dir, pics);
    assert_eq!(images[0].item_count, 2);

    let videos = list_buckets(&db, "video").unwrap();
    assert_eq!(videos.len(), 1);
    assert_eq!(videos[0].dir, vids);
    assert_eq!(videos[0].item_count, 1);

    assert!(list_buckets(&db, "audio").unwrap().is_empty());

    // Re-scanning the same tree must not double-count.
    s.stop_flag.store(0, Ordering::SeqCst);
    s.last_indexed.store(0, Ordering::SeqCst);
    let (seen2, _) = scan_tree(&db, dir.path(), &s);
    assert_eq!(seen2, 3);
    assert_eq!(list_buckets(&db, "image").unwrap()[0].item_count, 2);

    // Deleting one image decrements its bucket.
    let victim = get_by_path(&db, dir.path().join("Pics/a.jpg").to_str().unwrap())
        .unwrap()
        .unwrap();
    delete_by_uuid(&db, &victim.uuid).unwrap();
    assert_eq!(list_buckets(&db, "image").unwrap()[0].item_count, 1);
}

/// Perf harness for the rebuild hot path. Not run by default.
///   `cargo test --release scan_perf -- --ignored --nocapture`
/// File count via `SCAN_BENCH_FILES` (default 20_000).
///
/// Phase 1 (sequential per-file `scan_file`) is the pre-optimization
/// rebuild shape — one fjall commit per file. Phase 2 is the parallel
/// buffered engine (`scan_tree`).
#[test]
#[ignore]
fn scan_perf_bench() {
    let n: usize = std::env::var("SCAN_BENCH_FILES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(20_000);
    let dir = tree();
    let per_dir = 200usize;
    for d in 0..n.div_ceil(per_dir) {
        let sub = dir.path().join(format!("d{d:04}"));
        std::fs::create_dir_all(&sub).unwrap();
        for f in 0..per_dir.min(n - d * per_dir) {
            std::fs::write(sub.join(format!("f{f:03}.mp3")), b"x").unwrap();
        }
    }

    let db = tmp_db();
    let t = std::time::Instant::now();
    let total = count_files(dir.path());
    println!(
        "[bench] fs ceiling (readdir-only precount): {total} files in {:?} ({:.0} files/s)",
        t.elapsed(),
        total as f64 / t.elapsed().as_secs_f64().max(1e-9)
    );

    let t = std::time::Instant::now();
    let mut indexed = 0i64;
    for entry in crate::media::walk::Walk::new(dir.path())
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let p = entry.path().to_string_lossy().to_string();
        if scan_file(&db, &p).is_ok() {
            indexed += 1;
        }
    }
    let elapsed = t.elapsed();
    println!(
        "[bench] sequential scan_file: {indexed} files in {elapsed:?} ({:.0} files/s)",
        indexed as f64 / elapsed.as_secs_f64().max(1e-9)
    );
    assert_eq!(indexed as usize, n);

    reset_all(&db).unwrap();

    let s = scanner();
    s.stop_flag.store(0, Ordering::SeqCst);
    s.pause_flag.store(0, Ordering::SeqCst);
    s.last_indexed.store(0, Ordering::SeqCst);
    let db = Arc::new(db);
    let t = std::time::Instant::now();
    let (seen, idx) = scan_tree(&db, dir.path(), &s);
    let elapsed = t.elapsed();
    println!(
        "[bench] parallel scan_tree:   {idx} files (seen {seen}) in {elapsed:?} ({:.0} files/s)",
        idx as f64 / elapsed.as_secs_f64().max(1e-9)
    );
    assert_eq!(idx as usize, n);

    // Rows written by the buffered engine must be readable exactly like
    // per-file writes.
    let probe = dir.path().join("d0000/f000.mp3");
    assert!(get_by_path(&db, probe.to_str().unwrap()).unwrap().is_some());
}

#[test]
fn delete_by_path_drops_single_entry() {
    let dir = tree();
    let p = dir.path().join("a.mp3");
    std::fs::write(&p, b"x").unwrap();
    let db = tmp_db();
    scan_file(&db, p.to_str().unwrap()).unwrap();
    assert!(get_by_path(&db, p.to_str().unwrap()).unwrap().is_some());
    // Backslashes are normalized — Windows-style paths still hit the row.
    let alt = p.to_str().unwrap().replace('\\', "/");
    assert!(delete_by_path(&db, &alt).unwrap());
    assert!(get_by_path(&db, p.to_str().unwrap()).unwrap().is_none());
    // Calling again is a no-op (returns `false`) but still Ok.
    assert!(!delete_by_path(&db, &alt).unwrap());
}

#[test]
fn delete_by_path_prefix_purges_tree() {
    let dir = tree();
    let db = tmp_db();
    let root = dir.path().join("lib");
    std::fs::create_dir_all(root.join("sub/deep")).unwrap();
    for n in ["a.mp3", "sub/b.png", "sub/deep/c.mp4"] {
        let p = root.join(n);
        std::fs::write(&p, b"x").unwrap();
        scan_file(&db, p.to_str().unwrap()).unwrap();
    }
    let root_str = root.to_str().unwrap();
    // No-op on empty prefix.
    assert_eq!(delete_by_path_prefix(&db, "").unwrap(), 0);
    // Drop everything under the root.
    let purged = delete_by_path_prefix(&db, root_str).unwrap();
    assert_eq!(purged, 3);
    assert!(
        get_by_path(&db, &format!("{root_str}/a.mp3"))
            .unwrap()
            .is_none()
    );
    assert!(
        get_by_path(&db, &format!("{root_str}/sub/b.png"))
            .unwrap()
            .is_none()
    );
    assert!(
        get_by_path(&db, &format!("{root_str}/sub/deep/c.mp4"))
            .unwrap()
            .is_none()
    );
}

/// End-to-end: `rebuildMediaIndex` should publish the initial
/// "running" event (with `root` populated) **synchronously**, then
/// fire at least one 1-second-tick progress event before the scan
/// ends with an "idle" event. This is what the WebSocket hub
/// forwards to the frontend so the spinner + indexed/total numbers
/// update in real time. Mirrors `internal/graph/media_scan_api.go`'s
/// `rebuildMediaIndex` + `internal/media/scan.go`'s ticker.
///
/// Marked `#[ignore]` because it relies on the global
/// `SCANNER` singleton — running it concurrently with
/// `start_walk_and_scan_indexes_root` (which also drives the
/// singleton) produces a flaky race. Run with:
///   `cargo test -- --ignored rebuild_publishes_running_then_idle_progress_events`
#[tokio::test]
#[ignore]
async fn rebuild_publishes_running_then_idle_progress_events() {
    // Build a small library on disk.
    let dir = tree();
    std::fs::write(dir.path().join("a.mp3"), b"x").unwrap();
    std::fs::write(dir.path().join("b.png"), b"y").unwrap();

    // Subscribe to the progress channel BEFORE the rebuild fires
    // — mirrors the real-world ordering: WS hub connects and
    // subscribes, then the user clicks "rebuild".
    let events: std::sync::Arc<std::sync::Mutex<Vec<serde_json::Value>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let events2 = events.clone();
    let _sub = crate::media::eventbus::EventBus::global().subscribe(
        crate::media::eventbus::EVENT_MEDIA_SCAN_PROGRESS,
        move |payload: serde_json::Value| {
            events2.lock().unwrap().push(payload);
        },
    );

    // Reset global scanner state.
    scanner().stop();
    scanner().resume();
    scanner().last_indexed.store(0, Ordering::SeqCst);
    scanner().last_total.store(0, Ordering::SeqCst);
    *scanner().current_root.try_lock().unwrap() = String::new();

    // Replay the rebuild flow from `gql/mutation.rs`: synchronous
    // initial event + spawned heavy work.
    let root = dir.path().to_path_buf();
    let db = tmp_db();
    publish_initial_running(&root);
    let db2 = db.clone();
    let root2 = root.clone();
    tokio::spawn(async move {
        scanner().wait_for_running_task().await;
        let db3 = db2.clone();
        let _ = tokio::task::spawn_blocking(move || reset_all(&db3)).await;
        let _ = start_walk_and_scan(db2, root2).await;
    });

    // The initial "running" event is delivered synchronously.
    let initial = {
        let g = events.lock().unwrap();
        g.last().cloned().unwrap()
    };
    assert_eq!(initial["state"], "RUNNING");
    assert_eq!(initial["indexed"], 0);
    assert_eq!(initial["total"], 0);
    assert!(
        initial.get("root").is_some(),
        "running event must carry root"
    );

    // Wait for the walk to finish (state → idle). The test runs on
    // tokio's paused time but the walk uses spawn_blocking which
    // still uses real time for the count + per-file stat — we poll
    // for state transition with a short real-time sleep.
    for _ in 0..500 {
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        if scanner().state() == ScanState::Idle {
            break;
        }
    }
    assert_eq!(scanner().state(), ScanState::Idle, "scan should reach idle");

    // We should have seen at least: 1 initial running, ≥1 ticker
    // event (during/after precount), and 1 final idle. Even with
    // a tiny library the precount + walk takes long enough for the
    // ticker to fire at least once.
    let snapshot = events.lock().unwrap().clone();
    assert!(
        snapshot.len() >= 2,
        "expected at least 2 progress events, got {}: {snapshot:?}",
        snapshot.len()
    );
    // Last event must be the idle final event with the real `total`.
    let last = snapshot.last().unwrap();
    assert_eq!(last["state"], "IDLE");
    assert_eq!(last["total"], 2);
    assert_eq!(last["indexed"], 2);
    // The very first event must NOT have `total` set (running
    // event) — but intermediate ticker events DO. Just sanity check
    // that the `root` field is carried through.
    for ev in &snapshot {
        assert!(
            ev.get("root").is_some(),
            "every progress event while a scan runs must include root: {ev}"
        );
    }
}

// ---------------------------------------------------------------------------
// Read-side metadata hydration (duration/artist/title lazy probe + persist)
// ---------------------------------------------------------------------------
/// An audio row pointed at a real fixture with tags, but carrying none of
/// the probed metadata (what every scanned row looks like before hydration).
fn hydratable_audio(path: &str) -> MediaFile {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut h);
    let name = std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    MediaFile {
        uuid: format!("{:016x}", h.finish()),
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
        path: path.into(),
        original_path: path.into(),
        name,
        size: 1,
        modified_at: 7,
        r#type: "audio".into(),
        is_trash: false,
        trash_path: String::new(),
        deleted_at: 0,
    }
}

/// A search hit for a row, as the tantivy doc would present it pre-hydration.
fn bare_hit(mf: &MediaFile) -> crate::media::image_index::MediaSearchResult {
    crate::media::image_index::MediaSearchResult {
        uuid: mf.uuid.clone(),
        path: mf.path.clone(),
        name: mf.name.clone(),
        media_type: mf.r#type.clone(),
        size: mf.size,
        modified: mf.modified_at,
        duration_secs: 0,
        artist: String::new(),
        title: String::new(),
        is_trash: false,
    }
}

/// Counting probe seam: wraps the production probe, counts invocations.
/// The lock serializes tests that read the counter (parallel siblings in
/// this module would otherwise race the statics).
static PROBE_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static PROBE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn counting_probe(mf: &MediaFile) -> crate::media::metadata::ProbedMeta {
    PROBE_COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    probe_row(mf)
}

fn probe_counter() -> usize {
    PROBE_COUNT.load(std::sync::atomic::Ordering::SeqCst)
}

#[test]
fn probe_missing_metadata_probes_and_stamps_all_refs() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/audio-tagged.mp3");
    let mut mf = hydratable_audio(path);
    assert!(probe_missing_metadata(&mut mf));
    assert!(mf.duration_sec > 0);
    assert_eq!(mf.artist, "Hydrate Artist");
    assert_eq!(mf.title, "Hydrate Title");
    // All six ref stamps land: one probe covers the whole row.
    assert_eq!(mf.duration_ref_mod, 7);
    assert_eq!(mf.duration_ref_size, 1);
    assert_eq!(mf.artist_ref_mod, 7);
    assert_eq!(mf.artist_ref_size, 1);
    assert_eq!(mf.title_ref_mod, 7);
    assert_eq!(mf.title_ref_size, 1);
    // Fresh refs → no second probe.
    assert!(!probe_missing_metadata(&mut mf));
}

#[test]
fn probe_missing_metadata_failure_is_cached_not_retried() {
    // Negative caching: a probe that finds nothing (tagless file) still
    // stamps the refs, so the file is parsed at most once per (mtime,
    // size) — the Go Ensure* helpers re-parse such files on every read.
    let _g = PROBE_LOCK.lock().unwrap();
    PROBE_COUNT.store(0, std::sync::atomic::Ordering::SeqCst);
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/audio-untagged.mp3");
    let mut mf = hydratable_audio(path);
    assert!(probe_missing_metadata_with(&mut mf, counting_probe));
    assert_eq!(probe_counter(), 1);
    assert_eq!(mf.duration_sec, 4);
    assert_eq!(mf.artist, "");
    assert_eq!(mf.title, "");
    assert_eq!(mf.artist_ref_mod, 7, "empty result still stamps refs");
    // Second pass: refs are fresh → zero probes.
    assert!(!probe_missing_metadata_with(&mut mf, counting_probe));
    assert_eq!(probe_counter(), 1);
    // File changed (mtime+size) → refs stale → exactly one re-probe.
    mf.modified_at = 8;
    mf.size = 2;
    assert!(probe_missing_metadata_with(&mut mf, counting_probe));
    assert_eq!(probe_counter(), 2);
}

#[test]
fn probe_missing_metadata_missing_file_keeps_cached_values() {
    // A row whose file cannot be read (unmounted disk, moved file) must
    // keep its cached values — zeroing them on a failed probe would wipe
    // the library's metadata on the first page view — while still stamping
    // refs so the missing file is not re-probed on every view.
    let mut mf = hydratable_audio("/nonexistent/disk/song.mp3");
    mf.duration_sec = 42;
    mf.artist = "Cached".into();
    mf.title = "Cached Title".into();
    assert!(probe_missing_metadata(&mut mf));
    assert_eq!(mf.duration_sec, 42);
    assert_eq!(mf.artist, "Cached");
    assert_eq!(mf.title, "Cached Title");
    assert_eq!(mf.duration_ref_mod, 7, "failed probe still stamps refs");
    assert!(!probe_missing_metadata(&mut mf));
}

#[test]
fn probe_missing_metadata_audio_is_one_probe_not_three() {
    // The single-parse contract: one audio row missing duration+artist+title
    // costs exactly one probe (the naive ensure_* chain paid three parses).
    let _g = PROBE_LOCK.lock().unwrap();
    PROBE_COUNT.store(0, std::sync::atomic::Ordering::SeqCst);
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/audio-tagged.mp3");
    let mut mf = hydratable_audio(path);
    assert!(probe_missing_metadata_with(&mut mf, counting_probe));
    assert_eq!(probe_counter(), 1);
}

#[test]
fn probe_missing_metadata_skips_non_media_types() {
    let mut mf = hydratable_audio("/nope/a.jpg");
    mf.r#type = "image".into();
    assert!(!probe_missing_metadata(&mut mf));
    assert_eq!(mf.duration_sec, 0);
}

#[test]
fn hydrate_search_page_fills_missing_and_persists_kv_and_index() {
    let db = tmp_db();
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/audio-tagged.mp3");
    let mf = hydratable_audio(path);
    upsert_media_row(&db, &mf).unwrap();

    let mut hits = vec![bare_hit(&mf)];
    assert_eq!(hydrate_search_page(&db, &mut hits, true), 1);
    assert!(hits[0].duration_secs > 0);
    assert_eq!(hits[0].artist, "Hydrate Artist");
    assert_eq!(hits[0].title, "Hydrate Title");

    // Persisted: the KV row carries the probed values with fresh ref stamps,
    // so the next hydration pass is a no-op probe-wise.
    let row = get_by_uuid(&db, &mf.uuid).unwrap().unwrap();
    assert_eq!(row.duration_sec, hits[0].duration_secs);
    assert_eq!(row.artist, "Hydrate Artist");
    assert_eq!(row.duration_ref_mod, mf.modified_at);
    assert_eq!(row.duration_ref_size, mf.size);
    assert_eq!(hydrate_search_page(&db, &mut hits, true), 0);
}

#[test]
fn hydrate_search_page_video_hits_probe_duration_only() {
    let db = tmp_db();
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/testdata/video-h264-baseline.mp4"
    );
    let mut mf = hydratable_audio(path);
    mf.r#type = "video".into();
    upsert_media_row(&db, &mf).unwrap();

    let mut hits = vec![bare_hit(&mf)];
    // audio=false: videos need only duration; empty artist/title must not
    // block or trigger tag probing.
    assert_eq!(hydrate_search_page(&db, &mut hits, false), 1);
    assert!(hits[0].duration_secs > 0);
    let row = get_by_uuid(&db, &mf.uuid).unwrap().unwrap();
    assert_eq!(row.artist_ref_mod, 0, "video rows never probe tags");
}

#[test]
fn hydrate_search_page_skips_complete_hits_without_touching_kv() {
    let db = tmp_db();
    let _g = PROBE_LOCK.lock().unwrap();
    PROBE_COUNT.store(0, std::sync::atomic::Ordering::SeqCst);
    // A hit that already carries everything must be served without a KV
    // read/write and without a probe: with no row in the store, a
    // write-happy implementation would materialize one here.
    let mut hits = vec![crate::media::image_index::MediaSearchResult {
        uuid: "complete-hit".into(),
        path: "/x/a.mp3".into(),
        name: "a.mp3".into(),
        media_type: "audio".into(),
        size: 1,
        modified: 1,
        duration_secs: 6,
        artist: "A".into(),
        title: "T".into(),
        is_trash: false,
    }];
    assert_eq!(
        hydrate_search_page_with(&db, &mut hits, true, counting_probe),
        0
    );
    assert_eq!(probe_counter(), 0);
    assert_eq!(db.scan_prefix(b"media:uuid:").count(), 0);
}

#[test]
fn hydrate_search_page_steady_state_reprobes_nothing() {
    // The full steady-state contract: after the first view, a page of
    // tagless files serves from the row cache with zero probes, zero KV
    // writes (a probe-less pass persists nothing).
    let _g = PROBE_LOCK.lock().unwrap();
    PROBE_COUNT.store(0, std::sync::atomic::Ordering::SeqCst);
    let db = tmp_db();
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/audio-untagged.mp3");
    let mf = hydratable_audio(path);
    upsert_media_row(&db, &mf).unwrap();

    let mut hits = vec![bare_hit(&mf)];
    assert_eq!(
        hydrate_search_page_with(&db, &mut hits, true, counting_probe),
        1
    );
    assert_eq!(probe_counter(), 1);
    assert_eq!(hits[0].duration_secs, 4);

    // Second view: hit still value-incomplete (no tags in the doc), so the
    // row is read — but the fresh refs mean zero probes and zero writes.
    let mut hits = vec![bare_hit(&mf)];
    assert_eq!(
        hydrate_search_page_with(&db, &mut hits, true, counting_probe),
        0
    );
    assert_eq!(probe_counter(), 1);
    assert_eq!(hits[0].duration_secs, 4);
}

#[test]
fn hydrate_persist_never_rewrites_secondary_index_rows() {
    // Metadata hydration must not resurrect or rewrite the `media:path:` /
    // `media:fid:` secondaries: it writes exactly one KV row per file. Seed
    // a row WITH a fid, then remove both secondaries; after hydration they
    // must still be gone (a full re-upsert would have recreated them).
    let db = tmp_db();
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/audio-tagged.mp3");
    let mut mf = hydratable_audio(path);
    mf.fsuuid = "fs".into();
    mf.ino = 11;
    mf.ctime = 22;
    upsert_media_row(&db, &mf).unwrap();
    let fid = crate::media::uuid::fid_key("fs", 11, 22);
    assert!(
        db.get(format!("media:path:{path}").as_bytes())
            .unwrap()
            .is_some()
    );
    assert!(db.get(fid.as_bytes()).unwrap().is_some());

    let mut batch = db.batch();
    batch.remove(format!("media:path:{path}").as_bytes());
    batch.remove(fid.as_bytes());
    db.apply_batch(batch).unwrap();

    let mut mf2 = get_by_uuid(&db, &mf.uuid).unwrap().unwrap();
    assert!(hydrate_metadata(&db, &mut mf2));
    assert!(mf2.duration_sec > 0);
    assert!(
        db.get(format!("media:path:{path}").as_bytes())
            .unwrap()
            .is_none(),
        "metadata persist must not rewrite the path secondary"
    );
    assert!(
        db.get(fid.as_bytes()).unwrap().is_none(),
        "metadata persist must not rewrite the fid secondary"
    );
    // The row itself was updated in place.
    let row = get_by_uuid(&db, &mf.uuid).unwrap().unwrap();
    assert_eq!(row.duration_sec, mf2.duration_sec);
}

/// Quantified hydration benchmark (informational; NOT an assertion —
/// wall-clock never gates CI). Run with:
/// `cargo test --release hydrate_page_bench -- --ignored --nocapture`
#[test]
#[ignore]
fn hydrate_page_bench() {
    let db = tmp_db();
    let src = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/audio-tagged.mp3");
    // 30 copies under $HOME (tempdir under /var/folders is scan-excluded;
    // irrelevant here, but keeps paths realistic).
    let base = std::path::PathBuf::from(std::env::var("HOME").unwrap())
        .join(format!("plainnas-hydrate-bench-{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    let mut rows = Vec::new();
    for i in 0..30 {
        let p = base.join(format!("t{i}.mp3"));
        std::fs::copy(src, &p).unwrap();
        let mut mf = hydratable_audio(p.to_str().unwrap());
        mf.modified_at = 1000 + i;
        upsert_media_row(&db, &mf).unwrap();
        rows.push(mf);
    }
    let mut hits: Vec<_> = rows.iter().map(bare_hit).collect();

    let t = std::time::Instant::now();
    let n = hydrate_search_page(&db, &mut hits, true);
    let first = t.elapsed();
    assert_eq!(n, 30);

    let mut hits: Vec<_> = rows.iter().map(bare_hit).collect();
    let t = std::time::Instant::now();
    let n = hydrate_search_page(&db, &mut hits, true);
    let steady = t.elapsed();
    assert_eq!(n, 0);

    println!(
        "hydrate 30 mp3s: first view {first:?} ({:.1}/file), steady state {steady:?}",
        first.as_secs_f64() / 30.0
    );
    let _ = std::fs::remove_dir_all(&base);
}

// ----- Docs: classification, ext identity and the legacy-row migration -----

#[test]
fn infer_type_classifies_documents_off_shared_mime_table() {
    // text/* entries of the shared plain-rs table.
    for name in [
        "a.txt",
        "readme.log",
        "notes.md",
        "data.csv",
        "sheet.tsv",
        "page.html",
        "index.htm",
        "style.css",
        "script.js",
        "module.mjs",
    ] {
        assert_eq!(infer_type(name), "doc", "{name} must be a doc");
    }
    // plain-app extraDocumentMimeTypes beyond text/*.
    for name in ["report.pdf", "old.doc", "thesis.DOCX", "sheet.XLSX"] {
        assert_eq!(infer_type(name), "doc", "{name} must be a doc");
    }
    // Case-insensitive, same as the audio/video/image arms.
    assert_eq!(infer_type("RESUME.PDF"), "doc");

    // plain-app has no legacy .xls in extraDocumentMimeTypes — parity keeps
    // it out until the shared table decision changes on the phone side too.
    assert_eq!(infer_type("book.xls"), "other");
    assert_eq!(infer_type("app.json"), "other");
    assert_eq!(infer_type("conf.xml"), "other");

    // Non-docs stay as they were.
    assert_eq!(infer_type("song.mp3"), "audio");
    assert_eq!(infer_type("clip.mp4"), "video");
    assert_eq!(infer_type("photo.jpg"), "image");
    assert_eq!(infer_type("archive.zip"), "other");
    assert_eq!(infer_type("blob.bin"), "other");
    // Extensionless files have no extension-derived MIME and are not docs
    // (plain-app MediaStore assigns them application/octet-stream too).
    assert_eq!(infer_type("Makefile"), "other");
}

#[test]
fn ext_of_is_lowercased_last_extension() {
    assert_eq!(ext_of("Report.PDF"), "pdf");
    assert_eq!(ext_of("archive.tar.gz"), "gz");
    assert_eq!(ext_of("noext"), "");
    assert_eq!(ext_of(".hidden.md"), "md");
}
