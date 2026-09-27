//! Unit tests for `src/gql/mutation.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::sanitize_hostname;

/// Arc<Prefs> backed by a fresh prefs.json under `dir` (test fixture).
fn prefs(dir: &impl AsRef<std::path::Path>) -> std::sync::Arc<crate::prefs::Prefs> {
    std::sync::Arc::new(crate::prefs::Prefs::load(&dir.as_ref().join("prefs.json")).unwrap())
}

/// Test tree under `$HOME` — the default tempdir sits inside
/// `/var/folders/…/.tmpXXX`, which the media exclusions refuse to index.
fn tree() -> tempfile::TempDir {
    let base = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    tempfile::Builder::new()
        .prefix("plainnas-test-")
        .tempdir_in(base)
        .unwrap()
}

/// `db::tags` goes through the process-global default DB; make sure one
/// exists (idempotent — whichever test opens first wins, and the tag layer
/// stays self-consistent on it).
fn ensure_global_db() {
    let scratch = tree();
    let _ = crate::db::open(&scratch.path().join("fjall"));
}

#[test]
fn sanitize_strips_garbage() {
    assert_eq!(sanitize_hostname("My NAS!!"), "my-nas");
    assert_eq!(sanitize_hostname("  hello__world  "), "hello-world");
    assert_eq!(sanitize_hostname("---"), "");
    assert_eq!(sanitize_hostname("a.b.c"), "a-b-c");
}

/// `updateDeviceName` stores the plain-app display-name preference and
/// `App.deviceName` serves it back; a DB without the preference falls
/// back to the system hostname (`.ifEmpty { getDeviceName() }`).
#[tokio::test]
async fn update_device_name_serves_display_name_with_hostname_fallback() {
    let dir = tree();
    let db = std::sync::Arc::new(crate::db::Db::open(dir.path()).expect("temp db opens"));
    let config = std::sync::Arc::new(crate::config::Config::parse(
        "[server]\nhttp_port = 8080\nhttps_port = 8443\n",
    ));
    let schema = crate::gql::build_schema(
        db,
        prefs(&dir),
        config,
        dir.path().to_path_buf(),
        crate::test_support::chat_state(dir.path()),
    );

    // Fresh DB: no preference yet → system hostname.
    let resp = schema
        .execute(async_graphql::Request::new("{ app { deviceName } }"))
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    assert_eq!(
        resp.data.into_json().unwrap(),
        serde_json::json!({ "app": { "deviceName": plain_rs::utils::hostname::get() } })
    );

    // The plain-app mutation stores the display name (trimmed); the OS
    // hostname is left alone.
    let resp = schema
        .execute(
            async_graphql::Request::new(r#"mutation { updateDeviceName(name: "  media box  ") }"#)
                .data("test-client".to_string()),
        )
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    assert_eq!(
        resp.data.into_json().unwrap(),
        serde_json::json!({ "updateDeviceName": true })
    );

    let resp = schema
        .execute(async_graphql::Request::new("{ app { deviceName } }"))
        .await;
    assert_eq!(
        resp.data.into_json().unwrap(),
        serde_json::json!({ "app": { "deviceName": "media box" } })
    );
}

// ----- Tags (plain-app parity: relations keyed by media id) -----

/// `addToTags`/`removeFromTags` must resolve the shared DSL `query`
/// (`ids:…` for checkbox selections) into one relation per media id,
/// keyed by that id — the key every read path (media lists, `fileInfo`,
/// plain-app `TagsLoader`) looks up. The pre-fix code stored the raw query
/// string as a single key, so `.tags` always came back empty.
#[tokio::test]
async fn add_to_tags_resolves_ids_query_to_per_key_relations() {
    ensure_global_db();
    let dir = tree();
    let db = std::sync::Arc::new(crate::db::Db::open(dir.path()).unwrap());
    let config = std::sync::Arc::new(crate::config::Config::parse(
        "[server]\nhttp_port = 8080\nhttps_port = 8443\n",
    ));
    let schema = crate::gql::build_schema(
        db,
        prefs(&dir),
        config,
        dir.path().to_path_buf(),
        crate::test_support::chat_state(dir.path()),
    );

    let resp = schema
        .execute(async_graphql::Request::new(
            r#"mutation { createTag(type: AUDIO, name: "cardio") { id } }"#,
        ))
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    let tag_id = resp.data.into_json().unwrap()["createTag"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let resp = schema
        .execute(async_graphql::Request::new(format!(
            r#"mutation {{ addToTags(type: AUDIO, tagIds: ["{tag_id}"], query: "ids:S1,S2") }}"#
        )))
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    assert_eq!(
        resp.data.into_json().unwrap(),
        serde_json::json!({ "addToTags": true })
    );

    let library = plain_rs::library::db::LibraryDb::open(&dir.path().join("library.db")).unwrap();
    let mut keys = plain_rs::library::tags::keys_for_tag(&library, &tag_id);
    keys.sort();
    assert_eq!(keys, vec!["S1".to_string(), "S2".to_string()]);
    // plain-app semantics: the count reflects relations, one per media id.
    assert_eq!(
        plain_rs::library::tags::tag_by_id(&library, &tag_id)
            .unwrap()
            .count,
        2
    );

    // Re-adding is a no-op (plain-app skips keys the tag already has).
    schema
        .execute(async_graphql::Request::new(format!(
            r#"mutation {{ addToTags(type: AUDIO, tagIds: ["{tag_id}"], query: "ids:S1,S2") }}"#
        )))
        .await;
    assert_eq!(
        plain_rs::library::tags::keys_for_tag(&library, &tag_id).len(),
        2
    );

    // removeFromTags resolves the same way and removes per key.
    let resp = schema
        .execute(async_graphql::Request::new(format!(
            r#"mutation {{ removeFromTags(type: AUDIO, tagIds: ["{tag_id}"], query: "ids:S1") }}"#
        )))
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    let keys = plain_rs::library::tags::keys_for_tag(&library, &tag_id);
    assert_eq!(keys, vec!["S2".to_string()]);
}


// ---------------------------------------------------------------------------
// Developer pages (plain-app contract): clearAppLogs / deleteDataStoreEntry
// / deleteDbTableRows
// ---------------------------------------------------------------------------

/// `clearAppLogs` truncates `<data_dir>/logs/latest.log`.
#[tokio::test]
async fn clear_app_logs_truncates_log_file() {
    let dir = tree();
    let db = std::sync::Arc::new(crate::db::Db::open(dir.path()).expect("temp db opens"));
    let config = std::sync::Arc::new(crate::config::Config::parse(
        "[server]\nhttp_port = 8080\nhttps_port = 8443\n",
    ));
    let schema = crate::gql::build_schema(
        db,
        prefs(&dir),
        config,
        dir.path().to_path_buf(),
        crate::test_support::chat_state(dir.path()),
    );

    let log_file = crate::log::default_log_file(dir.path());
    std::fs::create_dir_all(log_file.parent().unwrap()).unwrap();
    std::fs::write(&log_file, b"stale line\n").unwrap();

    let resp = schema
        .execute(async_graphql::Request::new("mutation { clearAppLogs }"))
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    assert_eq!(std::fs::metadata(&log_file).unwrap().len(), 0);
}

/// `deleteDataStoreEntry` removes a preference (plain-desktop semantics:
/// any key in prefs.json, no namespace guard) and row data stays in fjall.
#[tokio::test]
async fn delete_data_store_entry_removes_preference() {
    let dir = tree();
    let db = std::sync::Arc::new(crate::db::Db::open(dir.path()).expect("temp db opens"));
    let config = std::sync::Arc::new(crate::config::Config::parse(
        "[server]\nhttp_port = 8080\nhttps_port = 8443\n",
    ));
    let prefs = prefs(&dir);
    prefs.set("device_name", "box").unwrap();
    db.insert(b"tag:row", b"v").unwrap();
    let schema = crate::gql::build_schema(
        db.clone(),
        prefs.clone(),
        config,
        dir.path().to_path_buf(),
        crate::test_support::chat_state(dir.path()),
    );

    let resp = schema
        .execute(async_graphql::Request::new(
            r#"mutation { deleteDataStoreEntry(key: "device_name") }"#,
        ))
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    assert_eq!(prefs.get::<String>("device_name").unwrap(), None);

    // Deleting an absent key is a quiet no-op true (plain-desktop parity).
    let resp = schema
        .execute(async_graphql::Request::new(
            r#"mutation { deleteDataStoreEntry(key: "absent") }"#,
        ))
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);

    // Row data is untouched by the preference delete.
    assert!(db.get(b"tag:row").unwrap().is_some());
}

/// `deleteDbTableRows` removes SQLite rows of the named table by primary
/// key, and rejects unknown tables / empty ids.
#[tokio::test]
async fn delete_db_table_rows_deletes_and_validates() {
    let dir = tree();
    let db = std::sync::Arc::new(crate::db::Db::open(dir.path()).expect("temp db opens"));
    let config = std::sync::Arc::new(crate::config::Config::parse(
        "[server]\nhttp_port = 8080\nhttps_port = 8443\n",
    ));
    let schema = crate::gql::build_schema(
        db.clone(),
        prefs(&dir),
        config,
        dir.path().to_path_buf(),
        crate::test_support::chat_state(dir.path()),
    );

    // Seed the library store build_schema opened (second handle).
    let library = plain_rs::library::db::LibraryDb::open(&dir.path().join("library.db")).unwrap();
    library.with_conn(|conn| {
        conn.execute_batch(
            "INSERT INTO tags(id, type, name) VALUES ('del-one', 1, 'a');
             INSERT INTO tags(id, type, name) VALUES ('del-two', 2, 'b');
             INSERT INTO tags(id, type, name) VALUES ('keep', 3, 'c');",
        )
        .unwrap();
    });

    // Unknown table rejected, nothing deleted.
    let resp = schema
        .execute(async_graphql::Request::new(
            r#"mutation { deleteDbTableRows(table: "media", ids: ["x"]) }"#,
        ))
        .await;
    assert!(!resp.errors.is_empty(), "unknown table must fail");
    assert_eq!(
        library.with_conn(|conn| conn
            .query_row("SELECT COUNT(*) FROM tags", [], |r| r.get::<_, i64>(0))
            .unwrap()),
        3
    );

    // Empty ids rejected.
    let resp = schema
        .execute(async_graphql::Request::new(
            r#"mutation { deleteDbTableRows(table: "tags", ids: []) }"#,
        ))
        .await;
    assert!(!resp.errors.is_empty(), "empty ids must fail");

    // In-table batch delete by primary key.
    let resp = schema
        .execute(async_graphql::Request::new(
            r#"mutation { deleteDbTableRows(table: "tags", ids: ["del-one", "del-two"]) }"#,
        ))
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    assert_eq!(
        library.with_conn(|conn| conn
            .query_row("SELECT id FROM tags", [], |r| r.get::<_, String>(0))
            .unwrap()),
        "keep"
    );
}

/// Clearing a log file that does not exist yet must still succeed and
/// leave an empty file behind (the standalone-truncate branch).
#[tokio::test]
async fn clear_app_logs_creates_missing_log_file() {
    let dir = tree();
    let db = std::sync::Arc::new(crate::db::Db::open(dir.path()).expect("temp db opens"));
    let config = std::sync::Arc::new(crate::config::Config::parse(
        "[server]\nhttp_port = 8080\nhttps_port = 8443\n",
    ));
    let schema = crate::gql::build_schema(
        db,
        prefs(&dir),
        config,
        dir.path().to_path_buf(),
        crate::test_support::chat_state(dir.path()),
    );

    let log_file = crate::log::default_log_file(dir.path());
    // In production `log::set_file` creates the `logs/` dir at startup;
    // mirror that, then exercise the file-missing branch.
    std::fs::create_dir_all(log_file.parent().unwrap()).unwrap();
    assert!(!log_file.exists(), "precondition: no log file");

    let resp = schema
        .execute(async_graphql::Request::new("mutation { clearAppLogs }"))
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    assert!(log_file.exists());
    assert_eq!(std::fs::metadata(&log_file).unwrap().len(), 0);
}

/// The plain-app audio playback surface, end to end through the schema:
/// playAudio enqueues + marks current + records history, the queue queries
/// mirror it, user playlists CRUD round-trips, and audioLyrics reads an
/// ID3v2 USLT frame written on disk.
#[tokio::test]
#[allow(clippy::await_holding_lock)] // GLOBAL_INDEX_TEST_LOCK serializes index writers on purpose
async fn audio_queue_and_user_playlists_plain_app_surface() {
    // scan_file/upsert write the process-global search index.
    let _guard = crate::gql::query::GLOBAL_INDEX_TEST_LOCK
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let dir = tree();
    let db = std::sync::Arc::new(crate::db::Db::open(dir.path()).expect("temp db opens"));
    let config = std::sync::Arc::new(crate::config::Config::parse(
        "[server]\nhttp_port = 8080\nhttps_port = 8443\n",
    ));
    let schema = crate::gql::build_schema(
        db.clone(),
        prefs(&dir),
        config,
        dir.path().to_path_buf(),
        crate::test_support::chat_state(dir.path()),
    );

    // An ID3-only file: audioLyrics must extract the USLT text, and the
    // unindexed playAudio falls back to the file stem for the title.
    let mut id3 = b"ID3".to_vec();
    let mut frame_body = vec![0];
    frame_body.extend_from_slice(b"eng");
    frame_body.push(0);
    frame_body.extend_from_slice(b"la la la lyrics");
    let mut frame = b"USLT".to_vec();
    frame.extend_from_slice(&((frame_body.len() as u32).to_be_bytes()));
    frame.extend_from_slice(&[0, 0]);
    frame.extend_from_slice(&frame_body);
    id3.extend_from_slice(&[3, 0, 0]);
    id3.extend_from_slice(&[
        ((frame.len() >> 21) & 0x7F) as u8,
        ((frame.len() >> 14) & 0x7F) as u8,
        ((frame.len() >> 7) & 0x7F) as u8,
        (frame.len() & 0x7F) as u8,
    ]);
    id3.extend_from_slice(&frame);
    let song = dir.path().join("road song.mp3");
    std::fs::write(&song, &id3).unwrap();
    let song_path = song.to_str().unwrap().to_string();

    async fn q(schema: &crate::gql::AppSchema, doc: &str) -> serde_json::Value {
        let resp = schema.execute(async_graphql::Request::new(doc)).await;
        assert!(resp.errors.is_empty(), "{:?} for {doc}", resp.errors);
        resp.data.into_json().unwrap()
    }
    let schema_ref = &schema;

    // playAudio: returns the track, marks it current, records history.
    let data = q(
        schema_ref,
        &format!(
            r#"mutation {{ playAudio(path: "{song_path}") {{ title artist path durationMs }} }}"#
        ),
    )
    .await;
    assert_eq!(data["playAudio"]["title"], "road song");
    assert_eq!(data["playAudio"]["path"], song_path);

    // A second track keeps queue order stable (append, dedup).
    let other = dir.path().join("zzz.mp3");
    std::fs::write(&other, b"x").unwrap();
    let other_path = other.to_str().unwrap().to_string();
    q(
        schema_ref,
        &format!(r#"mutation {{ playAudio(path: "{other_path}") {{ path }} }}"#),
    )
    .await;

    let data = q(
        schema_ref,
        r#"query { audioQueueItems(offset: 0, limit: 10, query: "") { path } audioQueueItemCount }"#,
    )
    .await;
    assert_eq!(data["audioQueueItems"][0]["path"], song_path);
    assert_eq!(data["audioQueueItems"][1]["path"], other_path);
    assert_eq!(data["audioQueueItemCount"], 2);

    // App serves the enum-typed play mode and the mirrored current track.
    // The NAS has no server-side transport, so isPlaying/positionMs serve
    // idle values even with a current track (API_SPEC §9).
    let data = q(
        schema_ref,
        r#"query { audioPlayback { mode currentPath isPlaying positionMs } }"#,
    )
    .await;
    assert_eq!(data["audioPlayback"]["mode"], "REPEAT");
    assert_eq!(data["audioPlayback"]["currentPath"], other_path);
    assert_eq!(data["audioPlayback"]["isPlaying"], false);
    assert_eq!(data["audioPlayback"]["positionMs"], 0);
    q(
        schema_ref,
        r#"mutation { updateAudioPlayMode(mode: SHUFFLE) }"#,
    )
    .await;
    let data = q(schema_ref, r#"query { audioPlayback { mode } }"#).await;
    assert_eq!(data["audioPlayback"]["mode"], "SHUFFLE");

    // Reorder reverses the manual queue.
    q(
        schema_ref,
        &format!(r#"mutation {{ reorderAudioQueue(paths: ["{other_path}", "{song_path}"]) }}"#),
    )
    .await;
    let data = q(
        schema_ref,
        r#"query { audioQueueItems(offset: 0, limit: 10, query: "") { path } }"#,
    )
    .await;
    assert_eq!(data["audioQueueItems"][0]["path"], other_path);

    // History: each playAudio recorded a play, newest first.
    let data = q(
        schema_ref,
        r#"query { audioPlayHistory(limit: 10, offset: 0, query: "") { path playCount } }"#,
    )
    .await;
    assert_eq!(data["audioPlayHistory"][0]["path"], other_path);
    assert_eq!(data["audioPlayHistory"][0]["playCount"], 1);
    assert_eq!(data["audioPlayHistory"].as_array().unwrap().len(), 2);

    // Lyrics straight from the ID3 frame.
    let data = q(
        schema_ref,
        &format!(r#"query {{ audioLyrics(path: "{song_path}") }}"#),
    )
    .await;
    assert_eq!(data["audioLyrics"], "la la la lyrics");

    // User playlists round-trip.
    let data = q(
        schema_ref,
        r#"mutation { createAudioPlaylist(name: "Focus") { id name itemCount } }"#,
    )
    .await;
    let pl_id = data["createAudioPlaylist"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(data["createAudioPlaylist"]["name"], "Focus");
    assert_eq!(data["createAudioPlaylist"]["itemCount"], 0);

    q(
        schema_ref,
        &format!(r#"mutation {{ addAudioPlaylistItems(id: "{pl_id}", paths: ["{song_path}", "{song_path}"]) }}"#),
    )
    .await;
    let data = q(
        schema_ref,
        &format!(r#"query {{ audioPlaylists {{ id name itemCount }} audioPlaylistItemCount(id: "{pl_id}") }}"#),
    )
    .await;
    assert_eq!(
        data["audioPlaylists"][0]["itemCount"], 1,
        "dup path ignored"
    );
    assert_eq!(data["audioPlaylistItemCount"], 1);
    let data = q(
        schema_ref,
        &format!(
            r#"query {{ audioPlaylistItems(id: "{pl_id}", offset: 0, limit: 10, query: "") {{ path }} }}"#
        ),
    )
    .await;
    assert_eq!(data["audioPlaylistItems"][0]["path"], song_path);

    q(
        schema_ref,
        &format!(r#"mutation {{ updateAudioPlaylist(id: "{pl_id}", name: "Deep focus") {{ id name itemCount }} }}"#),
    )
    .await;
    // Playing an empty-start playlist is still a valid call.
    q(
        schema_ref,
        &format!(r#"mutation {{ playAudioPlaylist(id: "{pl_id}", shuffle: false) {{ path }} }}"#),
    )
    .await;
    q(
        schema_ref,
        &format!(r#"mutation {{ removeAudioPlaylistItem(id: "{pl_id}", path: "{song_path}") }}"#),
    )
    .await;
    q(
        schema_ref,
        &format!(r#"mutation {{ deleteAudioPlaylist(id: "{pl_id}") }}"#),
    )
    .await;
    let data = q(schema_ref, r#"query { audioPlaylists { id } }"#).await;
    assert!(data["audioPlaylists"].as_array().unwrap().is_empty());

    // addAudiosToQueue with an explicit ids: query enqueues without the
    // search index (media row uuid → track).
    let mut mf = crate::media_scan::scan_file(&db, song.to_str().unwrap()).unwrap();
    mf.title = "Indexed".to_string();
    mf.artist = "Someone".to_string();
    mf.duration_sec = 42;
    crate::media_scan::upsert_media_row(&db, &mf).unwrap();
    q(
        schema_ref,
        &format!(
            r#"mutation {{ addAudiosToQueue(query: "ids:{}") }}"#,
            mf.uuid
        ),
    )
    .await;
    // playAudioPlaylist cleared the manual queue earlier (source switch),
    // so the ids-enqueued track is the only queue entry now.
    let data = q(
        schema_ref,
        r#"query { audioQueueItems(offset: 0, limit: 10, query: "") { title artist durationMs } }"#,
    )
    .await;
    let items = data["audioQueueItems"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["title"], "Indexed");
    assert_eq!(items[0]["artist"], "Someone");
    // Stored duration is seconds; the wire field is durationMs.
    assert_eq!(items[0]["durationMs"], serde_json::json!(42_000));

    // removeAudioFromQueue on a path that is no longer queued is a no-op;
    // clear resets everything.
    q(
        schema_ref,
        &format!(r#"mutation {{ removeAudioFromQueue(path: "{other_path}") }}"#),
    )
    .await;
    let data = q(schema_ref, r#"query { audioQueueItemCount }"#).await;
    assert_eq!(data["audioQueueItemCount"], 1);

    q(schema_ref, "mutation { clearAudioQueue }").await;
    let data = q(
        schema_ref,
        r#"query { audioQueueItemCount audioPlayback { currentPath } }"#,
    )
    .await;
    assert_eq!(data["audioQueueItemCount"], 0);
    // Idle is null on the phone contract — the empty string is never sent.
    assert_eq!(
        data["audioPlayback"]["currentPath"],
        serde_json::Value::Null
    );
}

/// Favorite folders follow the phone contract: addressed by
/// `rootPath`/`fullPath`, every mutation returns the updated list.
#[tokio::test]
async fn favorite_folders_use_full_path_and_return_the_list() {
    let dir = tree();
    let db = std::sync::Arc::new(crate::db::Db::open(dir.path()).expect("temp db opens"));
    let config = std::sync::Arc::new(crate::config::Config::parse(
        "[server]\nhttp_port = 8080\nhttps_port = 8443\n",
    ));
    let schema = crate::gql::build_schema(
        db,
        prefs(&dir),
        config,
        dir.path().to_path_buf(),
        crate::test_support::chat_state(dir.path()),
    );

    let data = schema
        .execute(async_graphql::Request::new(
            r#"mutation { addFavoriteFolder(rootPath: "/mnt/disk", fullPath: "/mnt/disk/media") { rootPath fullPath alias } }"#,
        ))
        .await
        .data
        .into_json()
        .unwrap();
    assert_eq!(
        data["addFavoriteFolder"],
        serde_json::json!([{ "rootPath": "/mnt/disk", "fullPath": "/mnt/disk/media", "alias": null }])
    );

    let data = schema
        .execute(async_graphql::Request::new(
            r#"mutation { setFavoriteFolderAlias(fullPath: "/mnt/disk/media", alias: "Media") { fullPath alias } }"#,
        ))
        .await
        .data
        .into_json()
        .unwrap();
    assert_eq!(
        data["setFavoriteFolderAlias"],
        serde_json::json!([{ "fullPath": "/mnt/disk/media", "alias": "Media" }])
    );

    // Removing by full path drops the entry and returns the (now empty) list.
    let data = schema
        .execute(async_graphql::Request::new(
            r#"mutation { removeFavoriteFolder(fullPath: "/mnt/disk/media") { fullPath } }"#,
        ))
        .await
        .data
        .into_json()
        .unwrap();
    assert_eq!(data["removeFavoriteFolder"], serde_json::json!([]));
}

/// `deleteFiles` reports how many paths were actually removed (phone
/// `ActionResult` contract).
#[tokio::test]
#[allow(clippy::await_holding_lock)] // GLOBAL_INDEX_TEST_LOCK serializes index writers on purpose
async fn delete_files_returns_affected_count() {
    // deleteFiles purges media rows from the process-global search index.
    let _guard = crate::gql::query::GLOBAL_INDEX_TEST_LOCK
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let dir = tree();
    let db = std::sync::Arc::new(crate::db::Db::open(dir.path()).expect("temp db opens"));
    let config = std::sync::Arc::new(crate::config::Config::parse(
        "[server]\nhttp_port = 8080\nhttps_port = 8443\n",
    ));
    let schema = crate::gql::build_schema(
        db,
        prefs(&dir),
        config,
        dir.path().to_path_buf(),
        crate::test_support::chat_state(dir.path()),
    );

    let a = dir.path().join("a.txt");
    std::fs::write(&a, b"a").unwrap();
    let resp = schema
        .execute(async_graphql::Request::new(format!(
            r#"mutation {{ deleteFiles(paths: ["{}", "/plainnas/does-not-exist"]) {{ affectedCount }} }}"#,
            a.display()
        )))
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    assert_eq!(
        resp.data.into_json().unwrap()["deleteFiles"]["affectedCount"],
        serde_json::json!(1),
        "only the existing path counts"
    );
    assert!(!a.exists());
}

/// `mergeChunks` runs as a background job (plain-app contract): the mutation
/// returns STARTED immediately, `mergeStatus` polls until DONE/FAILED, and a
/// re-call while terminal replays the result idempotently.
#[tokio::test]
async fn merge_chunks_is_a_background_task_with_status_polling() {
    let base = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let dir = tempfile::Builder::new()
        .prefix("plainnas-merge-")
        .tempdir_in(&base)
        .unwrap();
    // PLAIN_NAS_DATA_DIR is pinned process-wide to a fixed dir (see
    // pin_test_data_dir) — never this test's TempDir, whose deletion at
    // test end would destroy the shared media index. The resolver reads
    // its data dir via AppPaths::detect(), so the chunks are seeded under
    // the pinned dir too.
    let data_dir = crate::consts::AppPaths::pin_test_data_dir();
    let db = std::sync::Arc::new(crate::db::Db::open(dir.path()).unwrap());
    let config = std::sync::Arc::new(crate::config::Config::parse(
        "[server]\nhttp_port = 8080\nhttps_port = 8443\n",
    ));
    let prefs =
        std::sync::Arc::new(crate::prefs::Prefs::load(&dir.path().join("prefs.json")).unwrap());
    let schema = crate::gql::build_schema(
        db.clone(),
        prefs,
        config,
        dir.path().to_path_buf(),
        crate::test_support::chat_state(dir.path()),
    );

    // Seed two chunks for file MRG1.
    let chunks = crate::chunked_upload::chunk_dir(&data_dir, "MRG1");
    std::fs::create_dir_all(&chunks).unwrap();
    std::fs::write(chunks.join("chunk_0"), b"ab").unwrap();
    std::fs::write(chunks.join("chunk_1"), b"cd").unwrap();
    let dest = dir.path().join("out.bin");

    let q = format!(
        r#"mutation {{ mergeChunks(fileId: "MRG1", totalChunks: 2, path: "{}", replace: true, totalSize: 4) {{ status }} }}"#,
        dest.display()
    );
    let resp = schema.execute(async_graphql::Request::new(q)).await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    let status = resp.data.into_json().unwrap()["mergeChunks"]["status"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        status == "STARTED" || status == "DONE" || status == "MERGING",
        "unexpected initial status: {status}"
    );

    // Poll mergeStatus until terminal (deterministic: the job is two tiny
    // chunk reads; a bounded poll budget is ample).
    let mut final_status = String::new();
    for _ in 0..200 {
        let resp = schema
            .execute(async_graphql::Request::new(
                r#"{ mergeStatus(fileId: "MRG1") { status value mergedSize error } }"#,
            ))
            .await;
        assert!(resp.errors.is_empty(), "{:?}", resp.errors);
        let task = resp.data.into_json().unwrap()["mergeStatus"].clone();
        let st = task["status"].as_str().unwrap().to_string();
        if st == "DONE" || st == "FAILED" {
            final_status = st.clone();
            if st == "DONE" {
                assert_eq!(
                    task["mergedSize"],
                    serde_json::json!(4),
                    "two 2-byte chunks"
                );
            }
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(final_status, "DONE", "merge job must finish");
    assert_eq!(std::fs::read(&dest).unwrap(), b"abcd");

    // Idempotent re-call replays the terminal state.
    let resp = schema.execute(async_graphql::Request::new(format!(
        r#"mutation {{ mergeChunks(fileId: "MRG1", totalChunks: 2, path: "{}", replace: true, totalSize: 4) {{ status value mergedSize }} }}"#,
        dest.display()
    ))).await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    let task = resp.data.into_json().unwrap()["mergeChunks"].clone();
    assert_eq!(task["status"], serde_json::json!("DONE"));
    assert_eq!(task["mergedSize"], serde_json::json!(4));
}

/// `BookmarkGroup.itemCount` is the live count of the bookmark store:
/// a freshly created group serves 0, additions are reflected, and
/// `updateBookmarkGroup` keeps the count while renaming.
#[tokio::test]
async fn bookmark_groups_serve_live_item_count() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static SEQ: AtomicUsize = AtomicUsize::new(0);

    let dir = tree();
    let db = std::sync::Arc::new(crate::db::Db::open(dir.path()).expect("temp db opens"));
    let config = std::sync::Arc::new(crate::config::Config::parse(
        "[server]\nhttp_port = 8080\nhttps_port = 8443\n",
    ));
    let schema = crate::gql::build_schema(
        db,
        prefs(&dir),
        config,
        dir.path().to_path_buf(),
        crate::test_support::chat_state(dir.path()),
    );
    let name = format!("cnt-{}", SEQ.fetch_add(1, Ordering::SeqCst));

    let resp = schema
        .execute(async_graphql::Request::new(format!(
            r#"mutation {{ createBookmarkGroup(name: "{name}") {{ id itemCount }} }}"#
        )))
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    let created = resp.data.into_json().unwrap()["createBookmarkGroup"].clone();
    assert_eq!(created["itemCount"], 0, "fresh group: {created}");
    let id = created["id"].as_str().expect("id is a string").to_string();

    let urls: Vec<String> = (0..2).map(|i| format!("https://cnt{i}.example")).collect();
    let chat_db = plain_rs::chat::db::ChatDb::open(&dir.path().join("chat.db")).unwrap();
    for u in &urls {
        let b = plain_rs::chat::db::bookmark::DBookmark::new(u, &id);
        plain_rs::chat::db::bookmark::insert_bookmark(&chat_db, &b);
    }

    let resp = schema
        .execute(async_graphql::Request::new(
            r#"query { bookmarkGroups { id itemCount } }"#,
        ))
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    let groups = resp.data.into_json().unwrap()["bookmarkGroups"].clone();
    let mine = groups
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["id"] == id.as_str())
        .expect("created group is listed");
    assert_eq!(mine["itemCount"], 2, "live count: {groups}");

    let resp = schema
        .execute(async_graphql::Request::new(format!(
            r#"mutation {{ updateBookmarkGroup(id: "{id}", name: "{name}-2", collapsed: true, sortOrder: 1) {{ itemCount }} }}"#
        )))
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    let updated = resp.data.into_json().unwrap()["updateBookmarkGroup"].clone();
    assert_eq!(
        updated["itemCount"], 2,
        "rename keeps the live count: {updated}"
    );
}



/// Schema-level lock of the §5 guard: every query-addressed bulk mutation
/// rejects a blank query with `bulk_query_required` before touching the
/// index (hermetic — no media data needed).
#[tokio::test]
async fn bulk_media_mutations_reject_blank_query() {
    let dir = tree();
    let db = std::sync::Arc::new(crate::db::Db::open(dir.path()).expect("temp db opens"));
    let config = std::sync::Arc::new(crate::config::Config::parse(
        "[server]\nhttp_port = 8080\nhttps_port = 8443\n",
    ));
    let schema = crate::gql::build_schema(
        db,
        prefs(&dir),
        config,
        dir.path().to_path_buf(),
        crate::test_support::chat_state(dir.path()),
    );
    for mutation in [
        r#"mutation { deleteMediaItems(type: IMAGE, query: "") { affectedCount } }"#,
        r#"mutation { trashMediaItems(type: IMAGE, query: "   ") { affectedCount } }"#,
        r#"mutation { restoreMediaItems(type: IMAGE, query: "") { affectedCount } }"#,
        r#"mutation { moveMediaItems(type: IMAGE, query: "", destDir: "/") { affectedCount } }"#,
    ] {
        let resp = schema.execute(async_graphql::Request::new(mutation)).await;
        assert!(
            !resp.errors.is_empty(),
            "blank query must be rejected: {mutation}"
        );
        assert!(
            resp.errors[0].message.contains("bulk_query_required"),
            "unexpected error for {mutation}: {:?}",
            resp.errors[0].message
        );
    }
}

// ----- Chat mutations (plain_rs::chat backed) -----

fn chat_schema(
    dir: &tempfile::TempDir,
) -> (
    crate::gql::AppSchema,
    std::sync::Arc<crate::chat::ChatState>,
) {
    let db = std::sync::Arc::new(crate::db::Db::open(dir.path()).expect("temp db opens"));
    let config = std::sync::Arc::new(crate::config::Config::parse(
        "[server]\nhttp_port = 8080\nhttps_port = 8443\n",
    ));
    let chat = crate::test_support::chat_state(dir.path());
    let schema = crate::gql::build_schema(
        db,
        prefs(&dir),
        config,
        dir.path().to_path_buf(),
        chat.clone(),
    );
    (schema, chat)
}

#[tokio::test]
async fn chat_channel_lifecycle_through_graphql() {
    let dir = tree();
    let (schema, chat) = chat_schema(&dir);

    // Create: owner is a JOINED member with a channel key.
    let resp = schema
        .execute(r#"mutation { createChatChannel(name: " Team ") { id name ownerId members { peerId status } version status } }"#)
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    let v: serde_json::Value = resp.data.into_json().unwrap();
    let ch = &v["createChatChannel"];
    let cid = ch["id"].as_str().unwrap().to_string();
    assert_eq!(ch["name"], "Team");
    let owner = chat.service.identity.client_id.clone();
    assert_eq!(ch["ownerId"], owner);
    assert_eq!(ch["members"][0]["status"], "JOINED");
    assert_eq!(ch["version"], 1);
    assert_eq!(ch["status"], "JOINED");

    // Rename bumps the version.
    let resp = schema
        .execute(
            r#"mutation { updateChatChannel(id: "MSG1", name: "x") { id } }"#.replace("MSG1", &cid),
        )
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);

    // Unknown channel errors surface as GraphQL errors.
    let resp = schema
        .execute(r#"mutation { updateChatChannel(id: "nope", name: "x") { id } }"#)
        .await;
    assert!(
        resp.errors
            .iter()
            .any(|e| e.message.contains("Channel not found"))
    );
}

#[tokio::test]
async fn send_and_delete_local_chat_items() {
    let dir = tree();
    let (schema, chat) = chat_schema(&dir);

    let resp = schema
        .execute(r#"mutation { sendChatItem(target: "local", content: "{\"type\":\"TEXT\",\"value\":{\"text\":\"note\"}}") { id fromId toId status } }"#)
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    let v: serde_json::Value = resp.data.into_json().unwrap();
    let item = &v["sendChatItem"][0];
    // Contract: `me` when this device created the item.
    assert_eq!(item["fromId"], "me");
    assert_eq!(item["toId"], "local");
    assert_eq!(item["status"], "SENT");
    let id = item["id"].as_str().unwrap().to_string();

    // Blank bulk query is a no-op returning affectedCount 0 (§5).
    let resp = schema
        .execute(r#"mutation { deleteChatItems(query: "") { affectedCount } }"#)
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    assert_eq!(
        resp.data.into_json().unwrap()["deleteChatItems"]["affectedCount"],
        0
    );

    // ids: query deletes and reports the count.
    let resp = schema
        .execute(format!(
            r#"mutation {{ deleteChatItems(query: "ids:{id}") {{ affectedCount }} }}"#
        ))
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    assert_eq!(
        resp.data.into_json().unwrap()["deleteChatItems"]["affectedCount"],
        1
    );
    assert!(chat.service.db.get_chat_by_id(&id).is_none());

    // Deleting an unknown single item returns false, not an error.
    let resp = schema
        .execute(r#"mutation { deleteChatItem(id: "zzz") }"#)
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    assert_eq!(resp.data.into_json().unwrap()["deleteChatItem"], false);
}

#[tokio::test]
async fn peer_mutations_map_to_service_semantics() {
    let dir = tree();
    let (schema, chat) = chat_schema(&dir);

    // Unknown peer → false; a channel-member peer → demoted not deleted.
    let resp = schema
        .execute(r#"mutation { deletePeer(id: "ghost") }"#)
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    assert_eq!(resp.data.into_json().unwrap()["deletePeer"], false);

    use plain_rs::chat::db::DPeer;
    use plain_rs::chat::enums::{DeviceType, PeerStatus};
    chat.service.db.upsert_peer(&DPeer::new(
        "p1",
        "Pixel",
        "203.0.113.9",
        2443,
        DeviceType::Phone,
    ));

    let resp = schema.execute(r#"mutation { unpairPeer(id: "p1") }"#).await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    assert_eq!(resp.data.into_json().unwrap()["unpairPeer"], true);
    assert_eq!(
        chat.service.db.get_peer_by_id("p1").unwrap().status,
        PeerStatus::Unpaired
    );

    let resp = schema.execute(r#"mutation { deletePeer(id: "p1") }"#).await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    assert_eq!(resp.data.into_json().unwrap()["deletePeer"], true);
    assert!(chat.service.db.get_peer_by_id("p1").is_none());
}

#[tokio::test]
async fn pair_device_requires_an_ip() {
    let dir = tree();
    let (schema, _chat) = chat_schema(&dir);

    let resp = schema
        .execute(r#"mutation { pairDevice(input: { id: "d1", name: "Phone", ips: [], port: 2443, deviceType: PHONE, version: "1", platform: "android", lastSeen: "2026-01-01T00:00:00Z", discoveryMethods: [LAN] }) }"#)
        .await;
    assert!(resp.errors.iter().any(|e| e.message.contains("no ip")));
}

#[tokio::test]
async fn channel_system_message_stub_stays_noop() {
    let dir = tree();
    let (schema, _chat) = chat_schema(&dir);
    let resp = schema
        .execute(r#"mutation { channelSystemMessage(type: "INVITE", payload: "{}") }"#)
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    assert_eq!(
        resp.data.into_json().unwrap()["channelSystemMessage"],
        false
    );
}

#[tokio::test]
async fn merge_app_file_chunks_imports_into_content_addressed_store() {
    let base = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let dir = tempfile::Builder::new()
        .prefix("plainnas-merge-app-")
        .tempdir_in(&base)
        .unwrap();
    // Chunks are seeded under the pinned data dir (the resolver's
    // chunked_upload reads go through AppPaths::detect()).
    let pin_dir = crate::consts::AppPaths::pin_test_data_dir();
    let db = std::sync::Arc::new(crate::db::Db::open(dir.path()).unwrap());
    let config = std::sync::Arc::new(crate::config::Config::parse(
        "[server]\nhttp_port = 8080\nhttps_port = 8443\n",
    ));
    let prefs =
        std::sync::Arc::new(crate::prefs::Prefs::load(&dir.path().join("prefs.json")).unwrap());
    let chat = crate::test_support::chat_state(dir.path());
    let schema =
        crate::gql::build_schema(db, prefs, config, dir.path().to_path_buf(), chat.clone());

    let fid_key = format!("MRGAPP{}", std::process::id());
    let chunks = crate::chunked_upload::chunk_dir(&pin_dir, &fid_key);
    std::fs::create_dir_all(&chunks).unwrap();
    std::fs::write(chunks.join("chunk_0"), b"hello ").unwrap();
    std::fs::write(chunks.join("chunk_1"), b"attachment").unwrap();

    let q = format!(
        r#"mutation {{ mergeAppFileChunks(fileId: "{fid_key}", totalChunks: 2, fileName: "notes.txt", totalSize: 16) {{ status }} }}"#
    );
    let resp = schema.execute(async_graphql::Request::new(q)).await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);

    // Poll mergeStatus to terminal; DONE carries the fid suffix.
    let mut value = String::new();
    for _ in 0..200 {
        let resp = schema
            .execute(async_graphql::Request::new(format!(
                r#"{{ mergeStatus(fileId: "{fid_key}") {{ status value mergedSize }} }}"#
            )))
            .await;
        let v = resp.data.into_json().unwrap()["mergeStatus"].clone();
        match v["status"].as_str() {
            Some("DONE") => {
                value = v["value"].as_str().unwrap().to_string();
                assert_eq!(v["mergedSize"], 16);
                break;
            }
            Some("FAILED") => panic!("merge failed: {v}"),
            _ => tokio::time::sleep(std::time::Duration::from_millis(10)).await,
        }
    }
    assert!(
        value.ends_with(".txt"),
        "fid suffix keeps the name ext: {value}"
    );

    // The app_files row exists and the stored content round-trips.
    let hash = value.trim_end_matches(".txt").to_string();
    let row = chat.service.db.get_app_file(&hash).expect("app_files row");
    assert_eq!(row.size, 16);
    // The resolver imports through AppPaths::detect() (production: the
    // same dir ChatState uses; here the pinned test dir).
    let real = pin_dir.join(&row.real_path);
    assert_eq!(std::fs::read(&real).unwrap(), b"hello attachment");
    // Staging cleaned up.
    assert!(!chunks.exists());
}
