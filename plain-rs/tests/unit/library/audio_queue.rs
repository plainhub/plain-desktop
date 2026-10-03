//! Behavior locks for `src/library/audio_queue.rs` — ported 1:1 from
//! plain-nas's `tests/unit/db/audio_queue.rs` (the fjall implementation
//! these functions were moved from), with the tantivy index replaced by
//! the deterministic `FakeLibrary`.

use super::*;
#[path = "fixtures.rs"]
mod fixtures;
use fixtures::*;

// ---------------------------------------------------------------------------
// Manual queue
// ---------------------------------------------------------------------------

#[test]
fn enqueue_appends_moves_and_never_duplicates() {
    let db = test_db("enqueue");
    enqueue(
        &db,
        &[audio("/tq1/a.mp3", "A", 1), audio("/tq1/b.mp3", "B", 2)],
        false,
    )
    .unwrap();
    // Re-enqueueing an existing path re-appends it (dedup, no duplicate).
    enqueue(
        &db,
        &[audio("/tq1/b.mp3", "B", 2), audio("/tq1/c.mp3", "C", 3)],
        false,
    )
    .unwrap();
    let queued = crate::db::audio_queue::all_queue_items(&db).unwrap();
    assert_eq!(
        queued.iter().map(|q| q.path.as_str()).collect::<Vec<_>>(),
        ["/tq1/a.mp3", "/tq1/b.mp3", "/tq1/c.mp3"]
    );
    // play_next inserts at the front.
    enqueue(&db, &[audio("/tq1/z.mp3", "Z", 9)], true).unwrap();
    let queued = crate::db::audio_queue::all_queue_items(&db).unwrap();
    assert_eq!(queued[0].path, "/tq1/z.mp3");
    // Positions stay dense.
    for (i, q) in queued.iter().enumerate() {
        assert_eq!(q.sort_order, i as i64);
    }
}

#[test]
fn reorder_puts_known_first_and_keeps_unknown_at_end() {
    let db = test_db("reorder");
    enqueue(
        &db,
        &[
            audio("/tq2/a.mp3", "A", 1),
            audio("/tq2/b.mp3", "B", 2),
            audio("/tq2/c.mp3", "C", 3),
        ],
        false,
    )
    .unwrap();
    reorder_queued(
        &db,
        &[
            "/tq2/c.mp3".to_string(),
            "/tq2/absent.mp3".to_string(),
            "/tq2/a.mp3".to_string(),
        ],
    )
    .unwrap();
    let queued = crate::db::audio_queue::all_queue_items(&db).unwrap();
    assert_eq!(
        queued.iter().map(|q| q.path.as_str()).collect::<Vec<_>>(),
        ["/tq2/c.mp3", "/tq2/a.mp3", "/tq2/b.mp3"]
    );
}

#[test]
fn remove_paths_prunes_queue_history_playlist_items_and_current() {
    let db = test_db("remove_paths");
    enqueue(
        &db,
        &[audio("/tq3/a.mp3", "A", 1), audio("/tq3/b.mp3", "B", 2)],
        false,
    )
    .unwrap();
    let pl = create_playlist(&db, "PL").unwrap();
    add_playlist_items(
        &db,
        &pl.id,
        &[audio("/tq3/a.mp3", "A", 1), audio("/tq3/keep.mp3", "K", 5)],
    )
    .unwrap();
    on_playing(&db, "/tq3/stay.mp3", "S", "A", 9).unwrap();
    // a.mp3 plays last → it is the current track when removed.
    on_playing(&db, "/tq3/a.mp3", "A", "A", 1).unwrap();

    remove_paths(&db, &["/tq3/a.mp3".to_string()]).unwrap();

    assert!(
        crate::db::audio_queue::queue_item_by_path(&db, "/tq3/a.mp3")
            .unwrap()
            .is_none()
    );
    assert!(
        crate::db::audio_queue::history_by_path(&db, "/tq3/a.mp3")
            .unwrap()
            .is_none()
    );
    assert_eq!(playlist_item_count(&db, &pl.id).unwrap(), 1);
    // The current track was removed → cleared.
    assert_eq!(get_audio_current(&db).unwrap(), "");
    assert!(
        crate::db::audio_queue::queue_item_by_path(&db, "/tq3/b.mp3")
            .unwrap()
            .is_some()
    );
}

// ---------------------------------------------------------------------------
// User playlists
// ---------------------------------------------------------------------------

#[test]
fn playlist_crud_items_and_counts() {
    let db = test_db("playlist_crud");
    let pl = create_playlist(&db, "Roadtrip").unwrap();
    assert_eq!(pl.name, "Roadtrip");
    assert_eq!(
        add_playlist_items(
            &db,
            &pl.id,
            &[
                audio("/tq5/a.mp3", "A", 1),
                audio("/tq5/b.mp3", "B", 2),
                audio("/tq5/a.mp3", "A-dup", 1),
            ],
        )
        .unwrap(),
        2
    );
    assert_eq!(playlist_item_count(&db, &pl.id).unwrap(), 2);
    let page = playlist_items_page(&db, &pl.id, 1, 5, "").unwrap();
    assert_eq!(paths_of(&page), ["/tq5/b.mp3"]);

    let listed = playlists(&db).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].1, 2);
    assert_eq!(listed[0].0.name, "Roadtrip");

    rename_playlist(&db, &pl.id, "Roadtrip 2").unwrap();
    assert_eq!(
        playlist_by_id(&db, &pl.id).unwrap().unwrap().name,
        "Roadtrip 2"
    );

    assert!(rename_playlist(&db, "nope", "X").is_err());

    let other = create_playlist(&db, "Other").unwrap();
    // updated_at ordering: "Other" (touched later) first.
    assert_eq!(playlists(&db).unwrap()[0].0.id, other.id);

    remove_playlist_item(&db, &pl.id, "/tq5/a.mp3").unwrap();
    assert_eq!(playlist_item_count(&db, &pl.id).unwrap(), 1);

    delete_playlist(&db, &pl.id).unwrap();
    assert!(playlist_by_id(&db, &pl.id).unwrap().is_none());
    assert!(
        crate::db::audio_queue::playlist_items(&db, &pl.id)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn deleting_active_playlist_resets_source() {
    let db = test_db("delete_active_pl");
    let pl = create_playlist(&db, "P").unwrap();
    add_playlist_items(&db, &pl.id, &[audio("/tq6/a.mp3", "A", 1)]).unwrap();
    set_playlist_source(&db, &pl.id, None).unwrap();
    assert_eq!(
        active_playlist_id(&db).unwrap().as_deref(),
        Some(pl.id.as_str())
    );

    delete_playlist(&db, &pl.id).unwrap();
    assert_eq!(
        source(&db).unwrap().source,
        crate::db::audio_queue::QueueSourceKind::None
    );
    assert!(active_playlist_id(&db).unwrap().is_none());
}

// ---------------------------------------------------------------------------
// Play history
// ---------------------------------------------------------------------------

/// The DSL `text:` filter on the three paginated audio lists:
/// case-insensitive substring over title/artist/path, filtering before
/// offset/limit. Deterministic — fixed fixtures, no wall clock.
#[test]
fn text_filter_precedes_pagination_on_queue_playlist_and_history() {
    let db = test_db("text_filter");
    let mut lib = FakeLibrary::new(&[]);
    enqueue(
        &db,
        &[
            audio("/tf/road song.mp3", "Road Song", 1),
            audio("/tf/sea song.mp3", "Sea Song", 2),
            audio("/tf/zzz.mp3", "Zzz", 3),
        ],
        false,
    )
    .unwrap();

    // Queue: needle matches title case-insensitively.
    assert_eq!(
        paths_of(&queue_page(&db, &mut lib, 0, 10, "SONG").unwrap()),
        ["/tf/road song.mp3", "/tf/sea song.mp3"]
    );
    // Pagination applies after filtering.
    assert_eq!(
        paths_of(&queue_page(&db, &mut lib, 1, 1, "song").unwrap()),
        ["/tf/sea song.mp3"]
    );
    // Path is matched too; a non-matching needle yields nothing.
    assert_eq!(
        paths_of(&queue_page(&db, &mut lib, 0, 10, "/tf/road").unwrap()),
        ["/tf/road song.mp3"]
    );
    assert!(queue_page(&db, &mut lib, 0, 10, "nope").unwrap().is_empty());

    // Playlist items: same matching rules.
    let pl = create_playlist(&db, "mix").unwrap();
    add_playlist_items(
        &db,
        &pl.id,
        &[
            audio("/tf/road song.mp3", "Road Song", 1),
            audio("/tf/zzz.mp3", "Zzz", 3),
        ],
    )
    .unwrap();
    assert_eq!(
        paths_of(&playlist_items_page(&db, &pl.id, 0, 10, "road").unwrap()),
        ["/tf/road song.mp3"]
    );
    assert_eq!(
        playlist_items_page(&db, &pl.id, 0, 10, "").unwrap().len(),
        2
    );

    // History: artist substring matches, empty needle keeps everything.
    on_playing(&db, "/tf/road song.mp3", "Road Song", "Ferry", 1).unwrap();
    on_playing(&db, "/tf/zzz.mp3", "Zzz", "Other", 3).unwrap();
    let rows = history_page(&db, 0, 10, "ferry").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].path, "/tf/road song.mp3");
    assert_eq!(history_page(&db, 0, 10, "").unwrap().len(), 2);
}

#[test]
fn history_upserts_play_count_and_trims_to_keep() {
    let db = test_db("history_trim");
    on_playing(&db, "/tq7/a.mp3", "A", "X", 1).unwrap();
    on_playing(&db, "/tq7/a.mp3", "A2", "X", 2).unwrap();
    let h = crate::db::audio_queue::history_by_path(&db, "/tq7/a.mp3")
        .unwrap()
        .unwrap();
    assert_eq!(h.play_count, 2);
    assert_eq!(h.title, "A2");
    assert_eq!(h.duration_ms, 2);

    // Exceeding 5/4 × KEEP trims to KEEP newest; the trim runs on the
    // insert that crosses the threshold. 2 pre-existing rows + 252 inserts
    // end at 202 (the last insert lands at 202 ≤ 250, no further trim).
    for i in 0..(HISTORY_KEEP * 5 / 4 + 2) {
        on_playing(&db, &format!("/tq7/x{i}.mp3"), "T", "X", 1).unwrap();
    }
    assert_eq!(
        crate::db::audio_queue::all_history(&db).unwrap().len(),
        HISTORY_KEEP + 2
    );
    // The oldest inserted tracks are gone, the newest survive.
    let page = history_page(&db, 0, 10, "").unwrap();
    assert!(page[0].path.starts_with("/tq7/x"));
    assert!(
        crate::db::audio_queue::all_history(&db)
            .unwrap()
            .iter()
            .all(|r| !r.path.ends_with("a.mp3"))
    );
}

// ---------------------------------------------------------------------------
// Playback order (playlist source)
// ---------------------------------------------------------------------------

#[test]
fn queue_page_orders_head_manual_and_tail() {
    let db = test_db("order_head_tail");
    let mut lib = FakeLibrary::new(&[]);
    let pl = create_playlist(&db, "P").unwrap();
    add_playlist_items(
        &db,
        &pl.id,
        &[
            audio("/tq8/p1.mp3", "P1", 1),
            audio("/tq8/p2.mp3", "P2", 2),
            audio("/tq8/p3.mp3", "P3", 3),
            audio("/tq8/p4.mp3", "P4", 4),
        ],
    )
    .unwrap();
    // Start at p3: head p1..p3, then manual queue, then tail p4.
    set_playlist_source(&db, &pl.id, Some("/tq8/p3.mp3")).unwrap();
    enqueue(&db, &[audio("/tq8/m1.mp3", "M1", 9)], false).unwrap();

    let page = queue_page(&db, &mut lib, 0, 10, "").unwrap();
    assert_eq!(
        paths_of(&page),
        [
            "/tq8/p1.mp3",
            "/tq8/p2.mp3",
            "/tq8/p3.mp3",
            "/tq8/m1.mp3",
            "/tq8/p4.mp3"
        ]
    );
    assert_eq!(queue_total(&db, &mut lib).unwrap(), 5);

    // Paging windows map onto the same order.
    assert_eq!(
        paths_of(&queue_page(&db, &mut lib, 3, 2, "").unwrap()),
        ["/tq8/m1.mp3", "/tq8/p4.mp3"]
    );
    // set_playlist_source recorded the start track in history.
    assert!(
        crate::db::audio_queue::history_by_path(&db, "/tq8/p3.mp3")
            .unwrap()
            .is_some()
    );
    assert_eq!(get_audio_current(&db).unwrap(), "/tq8/p3.mp3");
}

#[test]
fn superseded_source_copy_hidden_from_queue() {
    let db = test_db("superseded");
    let mut lib = FakeLibrary::new(&[]);
    let pl = create_playlist(&db, "P").unwrap();
    add_playlist_items(
        &db,
        &pl.id,
        &[
            audio("/tq9/p1.mp3", "P1", 1),
            audio("/tq9/p2.mp3", "P2", 2),
            audio("/tq9/p3.mp3", "P3", 3),
        ],
    )
    .unwrap();
    set_playlist_source(&db, &pl.id, Some("/tq9/p1.mp3")).unwrap();
    // Manually queue the tail track: its source copy is superseded, the
    // manual slot plays — the queue renders it exactly once, right after
    // the head, and the superseded tail copy is hidden.
    enqueue(&db, &[audio("/tq9/p3.mp3", "P3", 3)], false).unwrap();
    let page = queue_page(&db, &mut lib, 0, 10, "").unwrap();
    assert_eq!(
        paths_of(&page),
        ["/tq9/p1.mp3", "/tq9/p3.mp3", "/tq9/p2.mp3"]
    );
    assert_eq!(queue_total(&db, &mut lib).unwrap(), 3);
}

#[test]
fn resolve_next_walks_order_and_skips_superseded() {
    let db = test_db("resolve_next");
    let mut lib = FakeLibrary::new(&[]);
    let pl = create_playlist(&db, "P").unwrap();
    add_playlist_items(
        &db,
        &pl.id,
        &[
            audio("/tqA/p1.mp3", "P1", 1),
            audio("/tqA/p2.mp3", "P2", 2),
            audio("/tqA/p3.mp3", "P3", 3),
        ],
    )
    .unwrap();
    set_playlist_source(&db, &pl.id, Some("/tqA/p2.mp3")).unwrap();
    // Manually queue a track that is NOT in the playlist: it plays right
    // after the head, then the source tail continues, then it wraps.
    enqueue(&db, &[audio("/tqA/m9.mp3", "M9", 9)], false).unwrap();

    let next = resolve_next(&db, &mut lib, true, false).unwrap().unwrap();
    assert_eq!(next.path, "/tqA/m9.mp3");
    let next2 = resolve_next(&db, &mut lib, true, false).unwrap().unwrap();
    assert_eq!(next2.path, "/tqA/p3.mp3");
    // Manual items always sit right after the current position: as the
    // current advances into the source, m9 comes up again before p2.
    let next3 = resolve_next(&db, &mut lib, true, false).unwrap().unwrap();
    assert_eq!(next3.path, "/tqA/m9.mp3");
    // Previous from the manual slot falls into the source tail (p3), then
    // further back through the source.
    let prev = resolve_next(&db, &mut lib, false, false).unwrap().unwrap();
    assert_eq!(prev.path, "/tqA/p3.mp3");
    let prev2 = resolve_next(&db, &mut lib, false, false).unwrap().unwrap();
    assert_eq!(prev2.path, "/tqA/p2.mp3");

    let pl2 = create_playlist(&db, "Q").unwrap();
    add_playlist_items(
        &db,
        &pl2.id,
        &[
            audio("/tqA/q1.mp3", "Q1", 1),
            audio("/tqA/q2.mp3", "Q2", 2),
            audio("/tqA/q3.mp3", "Q3", 3),
        ],
    )
    .unwrap();
    set_playlist_source(&db, &pl2.id, Some("/tqA/q2.mp3")).unwrap();
    enqueue(&db, &[audio("/tqA/q3.mp3", "Q3", 3)], false).unwrap();
    assert_eq!(
        resolve_next(&db, &mut lib, true, false)
            .unwrap()
            .unwrap()
            .path,
        "/tqA/q3.mp3"
    );
}

// ---------------------------------------------------------------------------
// Playback order (library source)
// ---------------------------------------------------------------------------

#[test]
fn library_source_resolves_and_pages_in_order() {
    let db = test_db("library_source");
    // FakeLibrary serves the given order under every sort.
    let mut lib = FakeLibrary::new(&[
        ("/tqB/t1.mp3", 10),
        ("/tqB/t2.mp3", 10),
        ("/tqB/t3.mp3", 10),
    ]);

    // No start path: first track.
    let start = set_library_source(&db, &mut lib, None, false, "DATE_DESC")
        .unwrap()
        .unwrap();
    assert_eq!(start.path, "/tqB/t1.mp3");

    // Locate by start path: t3 is last.
    set_library_source(&db, &mut lib, Some("/tqB/t3.mp3"), false, "DATE_DESC")
        .unwrap()
        .unwrap();
    let page = queue_page(&db, &mut lib, 0, 10, "").unwrap();
    assert_eq!(
        paths_of(&page),
        ["/tqB/t1.mp3", "/tqB/t2.mp3", "/tqB/t3.mp3"]
    );

    // Next from t3 wraps to the first again.
    let next = resolve_next(&db, &mut lib, true, false).unwrap().unwrap();
    assert_eq!(next.path, "/tqB/t1.mp3");

    // A manually queued library track supersedes its source copy: the tail
    // renders without it and the manual slot carries it right after the
    // current track (t1, position 0).
    enqueue(&db, &[audio("/tqB/t2.mp3", "T2", 2)], false).unwrap();
    assert_eq!(queue_total(&db, &mut lib).unwrap(), 3);
    assert_eq!(
        paths_of(&queue_page(&db, &mut lib, 0, 10, "").unwrap()),
        ["/tqB/t1.mp3", "/tqB/t2.mp3", "/tqB/t3.mp3"]
    );

    clear_queue(&db).unwrap();
    assert_eq!(queue_total(&db, &mut lib).unwrap(), 0);
    assert_eq!(get_audio_current(&db).unwrap(), "");
}

/// The cached library position is invalidated by a manual jump and
/// re-located lazily on the next sequential skip (`onPlaying` port).
#[test]
fn manual_jump_breaks_cached_library_position() {
    let db = test_db("manual_jump");
    let mut lib = FakeLibrary::new(&[("/tqF/a.mp3", 1), ("/tqF/b.mp3", 1), ("/tqF/c.mp3", 1)]);
    set_library_source(&db, &mut lib, Some("/tqF/b.mp3"), false, "DATE_DESC").unwrap();
    assert_eq!(source(&db).unwrap().current_index, 1);
    // Manual jump to c: index marked unknown.
    on_playing(&db, "/tqF/c.mp3", "C", "A", 1).unwrap();
    assert_eq!(source(&db).unwrap().current_index, -1);
    assert_eq!(source(&db).unwrap().current_path, "/tqF/c.mp3");
    // The next order computation re-locates c lazily and persists pos 2.
    queue_page(&db, &mut lib, 0, 10, "").unwrap();
    assert_eq!(source(&db).unwrap().current_index, 2);
    // Sequential skip from the last track wraps to the first (position 0).
    let next = resolve_next(&db, &mut lib, true, false).unwrap().unwrap();
    assert_eq!(next.path, "/tqF/a.mp3");
    assert_eq!(source(&db).unwrap().current_index, 0);
}

/// An unknown sort name degrades to the implementor's default without
/// touching the stored `sort_by` (the FakeLibrary ignores it; the point
/// is the core passes the raw string through and never fails).
#[test]
fn no_library_resolves_to_empty_everywhere() {
    let db = test_db("no_library");
    let mut lib = crate::library::audio_queue::NoLibrary;
    assert!(
        set_library_source(&db, &mut lib, None, false, "DATE_DESC")
            .unwrap()
            .is_none()
    );
    assert_eq!(queue_total(&db, &mut lib).unwrap(), 0);
    assert!(queue_page(&db, &mut lib, 0, 10, "").unwrap().is_empty());
    assert!(resolve_next(&db, &mut lib, true, false).unwrap().is_none());
}

// ---------------------------------------------------------------------------
// Track resolution fallback
// ---------------------------------------------------------------------------

#[test]
fn audio_track_from_path_stem() {
    let a = AudioTrack::from_path_stem("/music/My Song.mp3");
    assert_eq!(a.title, "My Song");
    assert_eq!(a.artist, "");
    assert_eq!(a.duration_ms, 0);
    assert_eq!(a.path, "/music/My Song.mp3");
    // Backslashes normalize to forward slashes.
    assert_eq!(AudioTrack::from_path_stem("\\a\\b.mp3").path, "/a/b.mp3");
}

// ---------------------------------------------------------------------------
// Play mode / current track
// ---------------------------------------------------------------------------

#[test]
fn mode_defaults_to_repeat_and_roundtrips() {
    let dir = tempfile::tempdir().unwrap();
    let prefs = crate::prefs::Prefs::load(&crate::prefs::default_path(dir.path())).unwrap();
    assert_eq!(get_audio_mode(&prefs).unwrap(), "REPEAT");
    save_audio_mode(&prefs, "SHUFFLE").unwrap();
    assert_eq!(get_audio_mode(&prefs).unwrap(), "SHUFFLE");
    save_audio_mode(&prefs, "  REPEAT_ONE ").unwrap();
    assert_eq!(get_audio_mode(&prefs).unwrap(), "REPEAT_ONE");
    let stored: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join("user_prefs.json")).unwrap())
            .unwrap();
    assert_eq!(stored["audio_play_mode"], 1);
    prefs.set_user("audio_play_mode", 2).unwrap();
    assert_eq!(get_audio_mode(&prefs).unwrap(), "SHUFFLE");
    prefs.set_user("audio_play_mode", 99).unwrap();
    assert!(get_audio_mode(&prefs).is_err());
    assert!(save_audio_mode(&prefs, "unsupported").is_err());
}

#[test]
fn current_track_lives_on_the_source_row() {
    let db = test_db("current_row");
    let src = crate::db::audio_queue::QueueSource {
        current_path: "/tqE/x.mp3".to_string(),
        ..Default::default()
    };
    save_source(&db, &src).unwrap();
    assert_eq!(get_audio_current(&db).unwrap(), "/tqE/x.mp3");
    // Reloaded from the same single row (plain-app DAudioQueueSource).
    assert_eq!(source(&db).unwrap().current_path, "/tqE/x.mp3");
}

/// Reopening the database file keeps everything (the tables are
/// `CREATE IF NOT EXISTS`, single source row upserted).
#[test]
fn state_survives_reopen() {
    let dir = std::env::temp_dir().join(format!("plain-rs-library-reopen-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("plain.db");
    {
        let db = crate::db::Db::open(&path).unwrap();
        let pl = create_playlist(&db, "Persist").unwrap();
        add_playlist_items(&db, &pl.id, &[audio("/p/a.mp3", "A", 1)]).unwrap();
        on_playing(&db, "/p/a.mp3", "A", "X", 1).unwrap();
    }
    let db = crate::db::Db::open(&path).unwrap();
    let listed = playlists(&db).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].1, 1);
    assert_eq!(history_page(&db, 0, 10, "").unwrap().len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn milliseconds_and_album_snapshot_survive_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audio.db");
    let db = crate::db::Db::open(&path).unwrap();
    let pl = create_playlist(&db, "Precise").unwrap();
    let mut item = audio("/precise.mp3", "Precise", 117_123);
    item.album_id = "album-42".into();
    add_playlist_items(&db, &pl.id, std::slice::from_ref(&item)).unwrap();
    enqueue(&db, std::slice::from_ref(&item), false).unwrap();
    on_playing(&db, &item.path, &item.title, &item.artist, item.duration_ms).unwrap();
    drop(db);
    let db = crate::db::Db::open(&path).unwrap();
    assert_eq!(
        crate::db::audio_queue::all_queue_items(&db).unwrap()[0].duration_ms,
        117_123
    );
    let saved = playlist_items_page(&db, &pl.id, 0, 10, "")
        .unwrap()
        .remove(0);
    assert_eq!(saved.duration_ms, 117_123);
    assert_eq!(saved.album_id, "album-42");
    assert_eq!(
        history_page(&db, 0, 10, "").unwrap()[0].duration_ms,
        117_123
    );
}

#[test]
fn duplicate_enqueue_uses_last_snapshot_and_preserves_other_tracks() {
    let db = test_db("queue_duplicates");
    enqueue(&db, &[audio("/old", "Old", 1)], false).unwrap();
    enqueue(
        &db,
        &[audio("/new", "First", 17), audio("/new", "Latest", 123)],
        true,
    )
    .unwrap();
    let rows = crate::db::audio_queue::all_queue_items(&db).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].title, "Latest");
    assert_eq!(rows[0].duration_ms, 123);
    assert_eq!(rows[1].path, "/old");
    reorder_queued(&db, &["/old".into(), "/old".into()]).unwrap();
    assert_eq!(
        crate::db::audio_queue::all_queue_items(&db).unwrap().len(),
        2
    );
}

#[test]
fn failed_queue_and_playlist_batch_writes_roll_back() {
    let db = test_db("rollback_batch");
    enqueue(&db, &[audio("/old", "Old", 1)], false).unwrap();
    db.with_conn(|c|c.execute_batch("CREATE TRIGGER reject_queue BEFORE INSERT ON audio_queue_items WHEN NEW.path='/bad' BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;")).unwrap();
    assert!(
        enqueue(
            &db,
            &[audio("/ok", "Ok", 2), audio("/bad", "Bad", 3)],
            false
        )
        .is_err()
    );
    assert_eq!(
        crate::db::audio_queue::all_queue_items(&db)
            .unwrap()
            .iter()
            .map(|r| r.path.as_str())
            .collect::<Vec<_>>(),
        vec!["/old"]
    );
    let pl = create_playlist(&db, "List").unwrap();
    db.with_conn(|c|c.execute_batch("CREATE TRIGGER reject_playlist BEFORE INSERT ON audio_playlist_items WHEN NEW.audio_path='/bad' BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;")).unwrap();
    assert!(
        add_playlist_items(
            &db,
            &pl.id,
            &[audio("/ok", "Ok", 2), audio("/bad", "Bad", 3)]
        )
        .is_err()
    );
    assert_eq!(playlist_item_count(&db, &pl.id).unwrap(), 0);
    assert!(add_playlist_items(&db, "unknown", &[audio("/ok", "Ok", 2)]).is_err());
}

#[test]
fn failed_playback_and_playlist_deletion_preserve_source() {
    let db = test_db("rollback_source");
    let pl = create_playlist(&db, "List").unwrap();
    add_playlist_items(
        &db,
        &pl.id,
        &[audio("/one", "One", 123), audio("/two", "Two", 456)],
    )
    .unwrap();
    set_playlist_source(&db, &pl.id, None).unwrap();
    let old = source(&db).unwrap();
    db.with_conn(|c|c.execute_batch("CREATE TRIGGER reject_history BEFORE INSERT ON audio_play_history BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;")).unwrap();
    assert!(on_playing(&db, "/two", "Two", "", 456).is_err());
    assert_eq!(source(&db).unwrap(), old);
    let mut library = NoLibrary;
    assert!(resolve_next(&db, &mut library, true, false).is_err());
    assert_eq!(source(&db).unwrap(), old);
    db.with_conn(|c|c.execute_batch("CREATE TRIGGER reject_delete BEFORE DELETE ON audio_playlist_items BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;")).unwrap();
    assert!(delete_playlist(&db, &pl.id).is_err());
    assert!(playlist_by_id(&db, &pl.id).unwrap().is_some());
    assert_eq!(source(&db).unwrap(), old);
}

#[test]
fn concurrent_queue_and_history_writes_do_not_lose_updates() {
    let db = test_db("concurrent_audio");
    let workers: Vec<_> = (0..16)
        .map(|i| {
            let db = db.clone();
            std::thread::spawn(move || {
                enqueue(&db, &[audio(&format!("/{i}"), "Track", 123)], false).unwrap();
                on_playing(&db, "/shared", "Track", "Artist", 123).unwrap();
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(
        crate::db::audio_queue::all_queue_items(&db).unwrap().len(),
        16
    );
    assert_eq!(
        crate::db::audio_queue::history_by_path(&db, "/shared")
            .unwrap()
            .unwrap()
            .play_count,
        16
    );
}

#[test]
fn failed_media_cleanup_rolls_back_every_audio_table() {
    let db = test_db("cleanup_failure");
    let pl = create_playlist(&db, "List").unwrap();
    add_playlist_items(&db, &pl.id, &[audio("/one", "One", 123)]).unwrap();
    set_playlist_source(&db, &pl.id, None).unwrap();
    enqueue(&db, &[audio("/one", "One", 123)], false).unwrap();
    db.with_conn(|c|c.execute_batch("CREATE TRIGGER reject_history_delete BEFORE DELETE ON audio_play_history BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;")).unwrap();
    assert!(remove_paths(&db, &["/one".into()]).is_err());
    assert_eq!(
        crate::db::audio_queue::all_queue_items(&db).unwrap().len(),
        1
    );
    assert_eq!(playlist_item_count(&db, &pl.id).unwrap(), 1);
    assert_eq!(source(&db).unwrap().current_path, "/one");
    db.with_conn(|c| c.execute_batch("DROP TRIGGER reject_history_delete;"))
        .unwrap();
    remove_paths(&db, &["/one".into()]).unwrap();
    assert_eq!(playlist_item_count(&db, &pl.id).unwrap(), 0);
    assert!(history_page(&db, 0, 10, "").unwrap().is_empty());
    assert!(source(&db).unwrap().current_path.is_empty());
}

#[test]
fn superseded_source_copies_are_removed_before_pagination() {
    let db = test_db("visible_pagination");
    let pl = create_playlist(&db, "List").unwrap();
    add_playlist_items(
        &db,
        &pl.id,
        &[
            audio("/a", "A", 1),
            audio("/b", "B", 2),
            audio("/c", "C", 3),
            audio("/d", "D", 4),
        ],
    )
    .unwrap();
    set_playlist_source(&db, &pl.id, None).unwrap();
    enqueue(&db, &[audio("/b", "B", 2)], true).unwrap();
    let mut library = NoLibrary;
    assert_eq!(
        paths_of(&queue_page(&db, &mut library, 0, 3, "").unwrap()),
        vec!["/a", "/b", "/c"]
    );
    assert_eq!(
        paths_of(&queue_page(&db, &mut library, 3, 3, "").unwrap()),
        vec!["/d"]
    );
    assert_eq!(
        paths_of(&queue_page(&db, &mut library, 0, 4, "").unwrap()),
        vec!["/a", "/b", "/c", "/d"]
    );
    assert_eq!(queue_total(&db, &mut library).unwrap(), 4);
}

#[test]
fn selecting_a_manual_track_retains_the_source_cursor_and_does_not_record_playback() {
    let db = test_db("manual_source_cursor");
    let mut lib = FakeLibrary::new(&[("/a", 1), ("/b", 2), ("/c", 3), ("/d", 4)]);
    select_library_source(&db, &mut lib, Some("/a"), false, "DATE_DESC").unwrap();
    enqueue(&db, &[audio("/d", "Manual", 4)], true).unwrap();
    assert_eq!(
        select_next(&db, &mut lib, true, false)
            .unwrap()
            .unwrap()
            .path,
        "/d"
    );
    assert_eq!(source(&db).unwrap().current_index, 0);
    assert_eq!(
        select_next(&db, &mut lib, true, false)
            .unwrap()
            .unwrap()
            .path,
        "/b"
    );
    assert!(history_page(&db, 0, 20, "").unwrap().is_empty());
    on_playing(&db, "/b", "B", "", 2).unwrap();
    assert_eq!(history_page(&db, 0, 20, "").unwrap()[0].play_count, 1);
}
