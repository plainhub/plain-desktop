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
    );
    // Re-enqueueing an existing path re-appends it (dedup, no duplicate).
    enqueue(
        &db,
        &[audio("/tq1/b.mp3", "B", 2), audio("/tq1/c.mp3", "C", 3)],
        false,
    );
    let queued = crate::library::db::audio_queue::all_queue_items(&db);
    assert_eq!(
        queued.iter().map(|q| q.path.as_str()).collect::<Vec<_>>(),
        ["/tq1/a.mp3", "/tq1/b.mp3", "/tq1/c.mp3"]
    );
    // play_next inserts at the front.
    enqueue(&db, &[audio("/tq1/z.mp3", "Z", 9)], true);
    let queued = crate::library::db::audio_queue::all_queue_items(&db);
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
    );
    reorder_queued(
        &db,
        &[
            "/tq2/c.mp3".to_string(),
            "/tq2/absent.mp3".to_string(),
            "/tq2/a.mp3".to_string(),
        ],
    );
    let queued = crate::library::db::audio_queue::all_queue_items(&db);
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
    );
    let pl = create_playlist(&db, "PL");
    add_playlist_items(
        &db,
        &pl.id,
        &[audio("/tq3/a.mp3", "A", 1), audio("/tq3/keep.mp3", "K", 5)],
    );
    on_playing(&db, "/tq3/stay.mp3", "S", "A", 9);
    // a.mp3 plays last → it is the current track when removed.
    on_playing(&db, "/tq3/a.mp3", "A", "A", 1);

    remove_paths(&db, &["/tq3/a.mp3".to_string()]);

    assert!(crate::library::db::audio_queue::queue_item_by_path(&db, "/tq3/a.mp3").is_none());
    assert!(crate::library::db::audio_queue::history_by_path(&db, "/tq3/a.mp3").is_none());
    assert_eq!(playlist_item_count(&db, &pl.id), 1);
    // The current track was removed → cleared.
    assert_eq!(get_audio_current(&db), "");
    assert!(crate::library::db::audio_queue::queue_item_by_path(&db, "/tq3/b.mp3").is_some());
}

// ---------------------------------------------------------------------------
// User playlists
// ---------------------------------------------------------------------------

#[test]
fn playlist_crud_items_and_counts() {
    let db = test_db("playlist_crud");
    let pl = create_playlist(&db, "Roadtrip");
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
        ),
        2
    );
    assert_eq!(playlist_item_count(&db, &pl.id), 2);
    let page = playlist_items_page(&db, &pl.id, 1, 5, "");
    assert_eq!(paths_of(&page), ["/tq5/b.mp3"]);

    let listed = playlists(&db);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].1, 2);
    assert_eq!(listed[0].0.name, "Roadtrip");

    rename_playlist(&db, &pl.id, "Roadtrip 2");
    assert_eq!(playlist_by_id(&db, &pl.id).unwrap().name, "Roadtrip 2");

    // Renaming a missing playlist is a silent no-op.
    rename_playlist(&db, "nope", "X");

    let other = create_playlist(&db, "Other");
    // updated_at ordering: "Other" (touched later) first.
    assert_eq!(playlists(&db)[0].0.id, other.id);

    remove_playlist_item(&db, &pl.id, "/tq5/a.mp3");
    assert_eq!(playlist_item_count(&db, &pl.id), 1);

    delete_playlist(&db, &pl.id);
    assert!(playlist_by_id(&db, &pl.id).is_none());
    assert!(crate::library::db::audio_queue::playlist_items(&db, &pl.id).is_empty());
}

#[test]
fn deleting_active_playlist_resets_source() {
    let db = test_db("delete_active_pl");
    let pl = create_playlist(&db, "P");
    add_playlist_items(&db, &pl.id, &[audio("/tq6/a.mp3", "A", 1)]);
    set_playlist_source(&db, &pl.id, None);
    assert_eq!(active_playlist_id(&db).as_deref(), Some(pl.id.as_str()));

    delete_playlist(&db, &pl.id);
    assert_eq!(
        source(&db).source,
        crate::library::db::audio_queue::QueueSourceKind::None
    );
    assert!(active_playlist_id(&db).is_none());
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
    );

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
    let pl = create_playlist(&db, "mix");
    add_playlist_items(
        &db,
        &pl.id,
        &[
            audio("/tf/road song.mp3", "Road Song", 1),
            audio("/tf/zzz.mp3", "Zzz", 3),
        ],
    );
    assert_eq!(
        paths_of(&playlist_items_page(&db, &pl.id, 0, 10, "road")),
        ["/tf/road song.mp3"]
    );
    assert_eq!(playlist_items_page(&db, &pl.id, 0, 10, "").len(), 2);

    // History: artist substring matches, empty needle keeps everything.
    on_playing(&db, "/tf/road song.mp3", "Road Song", "Ferry", 1);
    on_playing(&db, "/tf/zzz.mp3", "Zzz", "Other", 3);
    let rows = history_page(&db, 0, 10, "ferry");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].path, "/tf/road song.mp3");
    assert_eq!(history_page(&db, 0, 10, "").len(), 2);
}

#[test]
fn history_upserts_play_count_and_trims_to_keep() {
    let db = test_db("history_trim");
    on_playing(&db, "/tq7/a.mp3", "A", "X", 1);
    on_playing(&db, "/tq7/a.mp3", "A2", "X", 2);
    let h = crate::library::db::audio_queue::history_by_path(&db, "/tq7/a.mp3").unwrap();
    assert_eq!(h.play_count, 2);
    assert_eq!(h.title, "A2");
    assert_eq!(h.duration_secs, 2);

    // Exceeding 5/4 × KEEP trims to KEEP newest; the trim runs on the
    // insert that crosses the threshold. 2 pre-existing rows + 252 inserts
    // end at 202 (the last insert lands at 202 ≤ 250, no further trim).
    for i in 0..(HISTORY_KEEP * 5 / 4 + 2) {
        on_playing(&db, &format!("/tq7/x{i}.mp3"), "T", "X", 1);
    }
    assert_eq!(
        crate::library::db::audio_queue::all_history(&db).len(),
        HISTORY_KEEP + 2
    );
    // The oldest inserted tracks are gone, the newest survive.
    let page = history_page(&db, 0, 10, "");
    assert!(page[0].path.starts_with("/tq7/x"));
    assert!(
        crate::library::db::audio_queue::all_history(&db)
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
    let pl = create_playlist(&db, "P");
    add_playlist_items(
        &db,
        &pl.id,
        &[
            audio("/tq8/p1.mp3", "P1", 1),
            audio("/tq8/p2.mp3", "P2", 2),
            audio("/tq8/p3.mp3", "P3", 3),
            audio("/tq8/p4.mp3", "P4", 4),
        ],
    );
    // Start at p3: head p1..p3, then manual queue, then tail p4.
    set_playlist_source(&db, &pl.id, Some("/tq8/p3.mp3"));
    enqueue(&db, &[audio("/tq8/m1.mp3", "M1", 9)], false);

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
    assert!(crate::library::db::audio_queue::history_by_path(&db, "/tq8/p3.mp3").is_some());
    assert_eq!(get_audio_current(&db), "/tq8/p3.mp3");
}

#[test]
fn superseded_source_copy_hidden_from_queue() {
    let db = test_db("superseded");
    let mut lib = FakeLibrary::new(&[]);
    let pl = create_playlist(&db, "P");
    add_playlist_items(
        &db,
        &pl.id,
        &[
            audio("/tq9/p1.mp3", "P1", 1),
            audio("/tq9/p2.mp3", "P2", 2),
            audio("/tq9/p3.mp3", "P3", 3),
        ],
    );
    set_playlist_source(&db, &pl.id, Some("/tq9/p1.mp3"));
    // Manually queue the tail track: its source copy is superseded, the
    // manual slot plays — the queue renders it exactly once, right after
    // the head, and the superseded tail copy is hidden.
    enqueue(&db, &[audio("/tq9/p3.mp3", "P3", 3)], false);
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
    let pl = create_playlist(&db, "P");
    add_playlist_items(
        &db,
        &pl.id,
        &[
            audio("/tqA/p1.mp3", "P1", 1),
            audio("/tqA/p2.mp3", "P2", 2),
            audio("/tqA/p3.mp3", "P3", 3),
        ],
    );
    set_playlist_source(&db, &pl.id, Some("/tqA/p2.mp3"));
    // Manually queue a track that is NOT in the playlist: it plays right
    // after the head, then the source tail continues, then it wraps.
    enqueue(&db, &[audio("/tqA/m9.mp3", "M9", 9)], false);

    let next = resolve_next(&db, &mut lib, true, false).unwrap().unwrap();
    assert_eq!(next.path, "/tqA/m9.mp3");
    // The manual item is now current and is not in the source (currentPos
    // -1): the order becomes manual first, then the whole source.
    let next2 = resolve_next(&db, &mut lib, true, false).unwrap().unwrap();
    assert_eq!(next2.path, "/tqA/p1.mp3");
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

    // plain-app quirk parity: when the natural next track is one that was
    // manually queued AND exists in the source, the superseded-by-path walk
    // rejects both copies and resolveNext returns null (the player keeps
    // playing). Reproduced 1:1 from AudioQueueManager.resolveNext.
    let pl2 = create_playlist(&db, "Q");
    add_playlist_items(
        &db,
        &pl2.id,
        &[
            audio("/tqA/q1.mp3", "Q1", 1),
            audio("/tqA/q2.mp3", "Q2", 2),
            audio("/tqA/q3.mp3", "Q3", 3),
        ],
    );
    set_playlist_source(&db, &pl2.id, Some("/tqA/q2.mp3"));
    enqueue(&db, &[audio("/tqA/q3.mp3", "Q3", 3)], false);
    assert!(resolve_next(&db, &mut lib, true, false).unwrap().is_none());
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
    let start = set_library_source(&db, &mut lib, None, false)
        .unwrap()
        .unwrap();
    assert_eq!(start.path, "/tqB/t1.mp3");

    // Locate by start path: t3 is last.
    set_library_source(&db, &mut lib, Some("/tqB/t3.mp3"), false)
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
    enqueue(&db, &[audio("/tqB/t2.mp3", "T2", 2)], false);
    assert_eq!(queue_total(&db, &mut lib).unwrap(), 3);
    assert_eq!(
        paths_of(&queue_page(&db, &mut lib, 0, 10, "").unwrap()),
        ["/tqB/t1.mp3", "/tqB/t2.mp3", "/tqB/t3.mp3"]
    );

    clear_queue(&db);
    assert_eq!(queue_total(&db, &mut lib).unwrap(), 0);
    assert_eq!(get_audio_current(&db), "");
}

/// The cached library position is invalidated by a manual jump and
/// re-located lazily on the next sequential skip (`onPlaying` port).
#[test]
fn manual_jump_breaks_cached_library_position() {
    let db = test_db("manual_jump");
    let mut lib = FakeLibrary::new(&[("/tqF/a.mp3", 1), ("/tqF/b.mp3", 1), ("/tqF/c.mp3", 1)]);
    set_library_source(&db, &mut lib, Some("/tqF/b.mp3"), false).unwrap();
    assert_eq!(source(&db).current_index, 1);
    // Manual jump to c: index marked unknown.
    on_playing(&db, "/tqF/c.mp3", "C", "A", 1);
    assert_eq!(source(&db).current_index, -1);
    assert_eq!(source(&db).current_path, "/tqF/c.mp3");
    // The next order computation re-locates c lazily and persists pos 2.
    queue_page(&db, &mut lib, 0, 10, "").unwrap();
    assert_eq!(source(&db).current_index, 2);
    // Sequential skip from the last track wraps to the first (position 0).
    let next = resolve_next(&db, &mut lib, true, false).unwrap().unwrap();
    assert_eq!(next.path, "/tqF/a.mp3");
    assert_eq!(source(&db).current_index, 0);
}

/// An unknown sort name degrades to the implementor's default without
/// touching the stored `sort_by` (the FakeLibrary ignores it; the point
/// is the core passes the raw string through and never fails).
#[test]
fn no_library_resolves_to_empty_everywhere() {
    let db = test_db("no_library");
    let mut lib = crate::library::audio_queue::NoLibrary;
    assert!(
        set_library_source(&db, &mut lib, None, false)
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
    assert_eq!(a.duration_secs, 0);
    assert_eq!(a.path, "/music/My Song.mp3");
    // Backslashes normalize to forward slashes.
    assert_eq!(AudioTrack::from_path_stem("\\a\\b.mp3").path, "/a/b.mp3");
}

// ---------------------------------------------------------------------------
// Play mode / current track
// ---------------------------------------------------------------------------

#[test]
fn mode_defaults_to_repeat_and_roundtrips() {
    let db = test_db("mode");
    assert_eq!(get_audio_mode(&db), "REPEAT");
    save_audio_mode(&db, "SHUFFLE");
    assert_eq!(get_audio_mode(&db), "SHUFFLE");
    save_audio_mode(&db, "  REPEAT_ONE ");
    assert_eq!(get_audio_mode(&db), "REPEAT_ONE");
    // It is a library pref row, persisted in the SQLite file.
    assert_eq!(
        crate::library::db::audio_queue::get_pref(&db, "audio_play_mode").as_deref(),
        Some("REPEAT_ONE")
    );
}

#[test]
fn current_track_lives_on_the_source_row() {
    let db = test_db("current_row");
    let src = crate::library::db::audio_queue::QueueSource {
        current_path: "/tqE/x.mp3".to_string(),
        ..Default::default()
    };
    save_source(&db, &src);
    assert_eq!(get_audio_current(&db), "/tqE/x.mp3");
    // Reloaded from the same single row (plain-app DAudioQueueSource).
    assert_eq!(source(&db).current_path, "/tqE/x.mp3");
}

/// Reopening the database file keeps everything (the tables are
/// `CREATE IF NOT EXISTS`, single source row upserted).
#[test]
fn state_survives_reopen() {
    let dir = std::env::temp_dir().join(format!("plain-rs-library-reopen-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("library.db");
    {
        let db = crate::library::db::LibraryDb::open(&path).unwrap();
        let pl = create_playlist(&db, "Persist");
        add_playlist_items(&db, &pl.id, &[audio("/p/a.mp3", "A", 1)]);
        on_playing(&db, "/p/a.mp3", "A", "X", 1);
    }
    let db = crate::library::db::LibraryDb::open(&path).unwrap();
    let listed = playlists(&db);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].1, 1);
    assert_eq!(history_page(&db, 0, 10, "").len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}
