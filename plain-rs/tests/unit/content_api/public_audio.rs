use super::*;
use crate::content_api::host::Host;
use crate::content_api::public_schema::PublicSchema;
use crate::db::Db;
use crate::prefs::Prefs;
use serde_json::{Value, json};
use std::sync::Arc;

fn stub(host: Arc<Host>, handler: impl Fn(&str, Value) -> Value + Send + 'static) {
    let (generation, mut requests) = host.connect();
    let host = host.clone();
    tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            let Some(id) = request["id"].as_u64() else {
                continue;
            };
            let result = handler(
                &request["method"].as_str().unwrap_or_default(),
                request["params"].clone(),
            );
            let _ = host.reply(generation, json!({ "id": id, "result": result }));
        }
    });
}

const GRANTED: &str = r#"["WRITE_EXTERNAL_STORAGE"]"#;

fn fixture() -> (tempfile::TempDir, PublicSchema) {
    fixture_with(GRANTED, |method, params| match method {
        "systemAudioPlaylistTracks" => json!(params["paths"]
            .as_array()
            .unwrap()
            .iter()
            .map(|path| json!({
                "title": "Track", "artist": "Artist",
                "path": path.as_str().unwrap(), "durationMs": 1000,
            }))
            .collect::<Vec<_>>()),
        "systemAudioLibrarySort" => json!("DATE_DESC"),
        "systemAudioPlayMode" => json!("REPEAT"),
        "systemAudioPlaybackState" => json!({ "isPlaying": false, "positionMs": 0 }),
        "systemAudioPlay" | "systemAudioClear" => json!(true),
        "systemAudioSearchTracks" => json!([{
            "title": "Queued", "artist": "Artist", "path": "/music/one.mp3", "durationMs": 1000,
        }]),
        "systemMediaRows" => json!([]),
        "systemMediaCount" => json!(0),
        "systemMediaTagFacts" => json!([]),
        "systemAudioLyrics" => json!(""),
        // The playback order is the library plus the queue, so the library
        // half has to answer even when nothing is queued from it.
        "audioLibraryCount" => json!(0),
        "audioLibraryPath" => Value::Null,
        "audioLibraryPage" => json!([]),
        "audioLibraryLocate" => json!(-1),
        "audioLibraryContains" => json!(false),
        _other => panic!("unexpected host call {_other}"),
    })
}

fn fixture_with(
    permissions: &str,
    handler: impl Fn(&str, Value) -> Value + Send + 'static,
) -> (tempfile::TempDir, PublicSchema) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(Prefs::load(&dir.path().join("prefs.json")).unwrap());
    prefs
        .set(
            "api_permissions",
            serde_json::from_str::<Vec<String>>(permissions).unwrap(),
        )
        .unwrap();
    let db = Arc::new(Db::open(&dir.path().join("data.db")).unwrap());
    let host = Arc::new(Host::default());
    stub(host.clone(), handler);
    let directory = dir.path().to_path_buf();
    let (events, _) = tokio::sync::broadcast::channel(16);
    (
        dir,
        crate::content_api::public_schema::build(host, events, prefs, db, directory),
    )
}

fn database(schema: &PublicSchema) -> &Arc<Db> {
    schema.data::<Arc<Db>>().unwrap()
}

async fn query(schema: &PublicSchema, document: &str) -> Value {
    serde_json::to_value(schema.execute(document).await).unwrap()
}

fn track(path: &str, title: &str, artist: &str) -> AudioTrack {
    AudioTrack {
        path: path.to_string(),
        title: title.to_string(),
        artist: artist.to_string(),
        album_id: String::new(),
        duration_ms: 1000,
    }
}

fn seed_queue(schema: &PublicSchema, tracks: &[(&str, &str, &str)]) {
    let items: Vec<AudioTrack> = tracks
        .iter()
        .map(|(path, title, artist)| track(path, title, artist))
        .collect();
    domain::enqueue(database(schema), &items, false).unwrap();
}

/// A queue seeded behind the resolver's back still reads back through it:
/// that is the point of mounting the app's own service rather than a second
/// copy of the queue.
#[tokio::test]
async fn the_queue_is_paged_and_its_count_matches() {
    let (_dir, schema) = fixture();
    seed_queue(
        &schema,
        &[
            ("/music/a.mp3", "Alpha", "One"),
            ("/music/b.mp3", "Beta", "Two"),
            ("/music/c.mp3", "Gamma", "Three"),
        ],
    );

    let page = query(
        &schema,
        r#"query { audioQueueItems(offset: 0, limit: 2, query: "") { title artist path durationMs } }"#,
    )
    .await;
    let rows = page["data"]["audioQueueItems"].as_array().unwrap_or_else(|| panic!("{page}"));
    assert_eq!(rows.len(), 2, "{page}");
    assert_eq!(rows[0]["durationMs"], 1000);

    let count = query(&schema, r#"query { audioQueueItemCount }"#).await;
    assert_eq!(count["data"]["audioQueueItemCount"], 3);
}

/// `audioPlayback` merges the store and the player, so both halves are
/// asserted: the queue's current path and the platform's own progress.
#[tokio::test]
async fn playback_joins_the_queue_state_and_the_player_state() {
    let (_dir, schema) = fixture_with(GRANTED, |method, params| match method {
        "systemAudioPlaylistTracks" => json!(params["paths"]
            .as_array()
            .unwrap()
            .iter()
            .map(|path| json!({
                "title": "Track", "artist": "Artist",
                "path": path.as_str().unwrap(), "durationMs": 1000,
            }))
            .collect::<Vec<_>>()),
        "systemAudioPlayMode" => json!("SHUFFLE"),
        "systemAudioPlaybackState" => json!({ "isPlaying": true, "positionMs": 4200 }),
        _other => panic!("unexpected host call {_other}"),
    });
    database(&schema);
    query(&schema, r#"mutation { playAudio(path: "/music/one.mp3") { title path } }"#).await;

    let state = query(
        &schema,
        r#"query { audioPlayback { currentPath mode isPlaying positionMs } }"#,
    )
    .await;
    let playback = &state["data"]["audioPlayback"];
    assert_eq!(playback["currentPath"], "/music/one.mp3", "{state}");
    assert_eq!(playback["mode"], "SHUFFLE");
    assert_eq!(playback["isPlaying"], true);
    assert_eq!(playback["positionMs"], 4200);
}

/// Playing a track must mark it current without touching the queue (2026-09-27,
/// T58). `playAudio` used to append the track to the manual queue, and enqueue
/// moves an already-queued track to the tail — so every desktop play jumped the
/// clicked track to the end. Queueing is the caller's job: they combine
/// `addAudiosToQueue`/`enqueue` with the play.
#[tokio::test]
async fn playing_a_track_never_changes_the_queue() {
    let (_dir, schema) = fixture_with(GRANTED, |method, params| match method {
        "systemAudioPlaylistTracks" => json!([{
            "title": "Track", "artist": "Artist",
            "path": params["paths"][0].as_str().unwrap(), "durationMs": 1000,
        }]),
        _other => json!({}),
    });
    database(&schema);
    seed_queue(&schema, &[("/music/one.mp3", "One", "A"), ("/music/two.mp3", "Two", "B")]);

    let played = query(
        &schema,
        r#"mutation { playAudio(path: "/music/one.mp3") { path } }"#,
    )
    .await;
    assert_eq!(played["data"]["playAudio"]["path"], "/music/one.mp3", "{played}");

    let after = query(&schema, r#"query { audioQueueItems(offset: 0, limit: 10, query: "") { path } }"#).await;
    let paths: Vec<&str> = after["data"]["audioQueueItems"]
        .as_array()
        .unwrap_or_else(|| panic!("{after}"))
        .iter()
        .map(|item| item["path"].as_str().unwrap())
        .collect();
    assert_eq!(
        paths,
        vec!["/music/one.mp3", "/music/two.mp3"],
        "playAudio must leave the queue exactly as it was — got {paths:?}",
    );
}

/// The store keeps "no current track" as an empty string. The contract says
/// null, and an empty path is not a track a client can act on.
#[tokio::test]
async fn an_idle_player_reports_a_null_current_path() {
    let (_dir, schema) = fixture();
    let state = query(
        &schema,
        r#"query { audioPlayback { currentPath mode isPlaying positionMs } }"#,
    )
    .await;
    assert!(state["data"]["audioPlayback"]["currentPath"].is_null(), "{state}");
    assert_eq!(state["data"]["audioPlayback"]["mode"], "REPEAT");
}

#[tokio::test]
async fn empty_lyrics_are_null_rather_than_an_empty_string() {
    let (_dir, schema) = fixture_with(GRANTED, |method, _params| match method {
        "systemAudioLyrics" => json!(""),
        _other => panic!("unexpected host call {_other}"),
    });
    let none = query(&schema, r#"query { audioLyrics(path: "/music/a.mp3") }"#).await;
    assert!(none["data"]["audioLyrics"].is_null(), "{none}");

    let (_dir, schema) = fixture_with(GRANTED, |method, _params| match method {
        "systemAudioLyrics" => json!("la la la"),
        _other => panic!("unexpected host call {_other}"),
    });
    let some = query(&schema, r#"query { audioLyrics(path: "/music/a.mp3") }"#).await;
    assert_eq!(some["data"]["audioLyrics"], "la la la");
}

#[tokio::test]
async fn clearing_empties_the_queue_and_tells_the_player() {
    let (_dir, schema) = fixture();
    seed_queue(&schema, &[("/music/a.mp3", "Alpha", "One")]);
    let cleared = query(&schema, r#"mutation { clearAudioQueue }"#).await;
    assert_eq!(cleared["data"]["clearAudioQueue"], true, "{cleared}");

    let count = query(&schema, r#"query { audioQueueItemCount }"#).await;
    assert_eq!(count["data"]["audioQueueItemCount"], 0, "{count}");
}

#[tokio::test]
async fn reordering_writes_the_order_the_client_sent() {
    let (_dir, schema) = fixture();
    seed_queue(
        &schema,
        &[
            ("/music/a.mp3", "Alpha", "One"),
            ("/music/b.mp3", "Beta", "Two"),
            ("/music/c.mp3", "Gamma", "Three"),
        ],
    );
    let ok = query(
        &schema,
        r#"mutation { reorderAudioQueue(paths: ["/music/c.mp3", "/music/a.mp3", "/music/b.mp3"]) }"#,
    )
    .await;
    assert_eq!(ok["data"]["reorderAudioQueue"], true);

    let items = query(
        &schema,
        r#"query { audioQueueItems(offset: 0, limit: 10, query: "") { path } }"#,
    )
    .await;
    let paths: Vec<&str> = items["data"]["audioQueueItems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, vec!["/music/c.mp3", "/music/a.mp3", "/music/b.mp3"], "{items}");
}

#[tokio::test]
async fn removing_one_item_leaves_the_rest_of_the_queue() {
    let (_dir, schema) = fixture();
    seed_queue(
        &schema,
        &[("/music/a.mp3", "Alpha", "One"), ("/music/b.mp3", "Beta", "Two")],
    );
    let ok = query(
        &schema,
        r#"mutation { removeAudioFromQueue(path: "/music/a.mp3") }"#,
    )
    .await;
    assert_eq!(ok["data"]["removeAudioFromQueue"], true);

    let items = query(
        &schema,
        r#"query { audioQueueItems(offset: 0, limit: 10, query: "") { path } }"#,
    )
    .await;
    let paths: Vec<&str> = items["data"]["audioQueueItems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, vec!["/music/b.mp3"], "{items}");
}

#[tokio::test]
async fn a_playlist_is_created_renamed_and_empties_into_a_count() {
    let (_dir, schema) = fixture();
    let created = query(
        &schema,
        r#"mutation { createAudioPlaylist(name: "Focus") { id name itemCount } }"#,
    )
    .await;
    let playlist = &created["data"]["createAudioPlaylist"];
    let id = playlist["id"].as_str().unwrap().to_string();
    assert_eq!(playlist["name"], "Focus");
    assert_eq!(playlist["itemCount"], 0);

    let added = query(
        &schema,
        &format!(
            r#"mutation {{ addAudioPlaylistItems(id: "{id}", paths: ["/music/a.mp3", "/music/b.mp3"]) }}"#
        ),
    )
    .await;
    assert_eq!(added["data"]["addAudioPlaylistItems"], true, "{added}");

    let count = query(
        &schema,
        &format!(r#"query {{ audioPlaylistItemCount(id: "{id}") }}"#),
    )
    .await;
    assert_eq!(count["data"]["audioPlaylistItemCount"], 2, "{count}");

    let renamed = query(
        &schema,
        &format!(r#"mutation {{ updateAudioPlaylist(id: "{id}", name: "Deep focus") {{ name itemCount }} }}"#),
    )
    .await;
    assert_eq!(renamed["data"]["updateAudioPlaylist"]["name"], "Deep focus");
    // The update reports the row it changed, count included, so a client
    // never has to re-read the playlist to learn its size.
    assert_eq!(renamed["data"]["updateAudioPlaylist"]["itemCount"], 2);

    let listed = query(&schema, r#"query { audioPlaylists { id name itemCount } }"#).await;
    assert_eq!(listed["data"]["audioPlaylists"].as_array().map(Vec::len), Some(1));

    let _ = query(
        &schema,
        &format!(r#"mutation {{ deleteAudioPlaylist(id: "{id}") }}"#),
    )
    .await;
    let after = query(&schema, r#"query { audioPlaylists { id } }"#).await;
    assert_eq!(after["data"]["audioPlaylists"].as_array().map(Vec::len), Some(0));
}

#[tokio::test]
async fn removing_a_playlist_item_lowers_its_count() {
    let (_dir, schema) = fixture();
    let created = query(
        &schema,
        r#"mutation { createAudioPlaylist(name: "Mix") { id } }"#,
    )
    .await;
    let id = created["data"]["createAudioPlaylist"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let _ = query(
        &schema,
        &format!(r#"mutation {{ addAudioPlaylistItems(id: "{id}", paths: ["/music/a.mp3"]) }}"#),
    )
    .await;

    let removed = query(
        &schema,
        &format!(r#"mutation {{ removeAudioPlaylistItem(id: "{id}", path: "/music/a.mp3") }}"#),
    )
    .await;
    assert_eq!(removed["data"]["removeAudioPlaylistItem"], true, "{removed}");

    let count = query(
        &schema,
        &format!(r#"query {{ audioPlaylistItemCount(id: "{id}") }}"#),
    )
    .await;
    assert_eq!(count["data"]["audioPlaylistItemCount"], 0, "{count}");
}

/// An empty playlist has nothing to start, and the contract types the result
/// nullable for exactly that. An error would make "nothing to play" look
/// like a failure.
#[tokio::test]
async fn playing_an_empty_playlist_is_null_rather_than_an_error() {
    let (_dir, schema) = fixture();
    let created = query(
        &schema,
        r#"mutation { createAudioPlaylist(name: "Nothing") { id } }"#,
    )
    .await;
    let id = created["data"]["createAudioPlaylist"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let played = query(
        &schema,
        &format!(r#"mutation {{ playAudioPlaylist(id: "{id}", shuffle: false) {{ title }} }}"#),
    )
    .await;
    assert!(played["errors"].as_array().is_none_or(Vec::is_empty), "{played}");
    assert!(played["data"]["playAudioPlaylist"].is_null(), "{played}");
}

#[tokio::test]
async fn playing_the_library_is_null_when_nothing_matches() {
    let (_dir, schema) = fixture();
    let played = query(&schema, r#"mutation { playAllAudios(shuffle: false) { title } }"#).await;
    assert!(played["errors"].as_array().is_none_or(Vec::is_empty), "{played}");
    assert!(played["data"]["playAllAudios"].is_null(), "{played}");
}

#[tokio::test]
async fn play_mode_is_written_through_the_host_preference() {
    let (_dir, schema) = fixture();
    let ok = query(&schema, r#"mutation { updateAudioPlayMode(mode: REPEAT_ONE) }"#).await;
    assert_eq!(ok["data"]["updateAudioPlayMode"], true, "{ok}");

    let state = query(&schema, r#"query { audioPlayback { mode } }"#).await;
    assert_eq!(state["data"]["audioPlayback"]["mode"], "REPEAT");
}

/// Without storage the library degrades rather than erroring — the same rule
/// the image and video lists use — while an explicit browse still refuses.
#[tokio::test]
async fn an_ungranted_library_counts_zero_and_refuses_to_list() {
    let (_dir, schema) = fixture_with("[]", |method, _| match method {
        "systemMediaRows" => json!([{
            "id": "1", "title": "Hidden", "artist": "A", "path": "/m/a.mp3",
            "size": 1, "bucketId": "b", "durationMs": 1, "albumFileId": "",
            "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-01T00:00:00Z",
            "isFavorite": false,
        }]),
        _other => panic!("unexpected host call {_other}"),
    });

    let count = query(&schema, r#"query { audioCount(query: "") }"#).await;
    assert_eq!(count["data"]["audioCount"], 0, "{count}");

    let listed = query(
        &schema,
        r#"query { audios(offset: 0, limit: 10, query: "", sortBy: NAME_ASC) { title } }"#,
    )
    .await;
    assert!(!listed["errors"].as_array().is_none_or(Vec::is_empty), "{listed}");
}

#[tokio::test]
async fn a_library_row_keeps_the_album_art_reference() {
    let (_dir, schema) = fixture_with(GRANTED, |method, _params| match method {
        "systemMediaTagFacts" => json!([]),
        "systemMediaRows" => json!([{
            "id": "1", "title": "Song", "artist": "Band", "path": "/m/a.mp3",
            "size": 42, "bucketId": "bucket-1", "durationMs": 1234,
            "albumFileId": "album-file-1",
            "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-01T00:00:01Z",
            "isFavorite": true,
        }]),
        _other => panic!("unexpected host call {_other}"),
    });

    let listed = query(
        &schema,
        r#"query { audios(offset: 0, limit: 10, query: "", sortBy: NAME_ASC) {
            id title artist path durationMs size bucketId albumFileId createdAt updatedAt isFavorite tags { name }
        } }"#,
    )
    .await;
    let row = &listed["data"]["audios"][0];
    assert_eq!(row["albumFileId"], "album-file-1", "{listed}");
    assert_eq!(row["artist"], "Band");
    assert_eq!(row["durationMs"], 1234);
    assert_eq!(row["size"], 42);
    assert_eq!(row["bucketId"], "bucket-1");
    assert_eq!(row["isFavorite"], true);
    assert_eq!(row["tags"].as_array().map(Vec::len), Some(0));
}