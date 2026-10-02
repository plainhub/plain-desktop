//! Unit tests for the fixed-query managers in `src/db/managers/simple.rs`.

#[path = "../fixtures.rs"]
mod fixtures;
use crate::db::Db;
use base64::Engine;
use fixtures::*;

use crate::db::models::simple::{
    ArchivedConversationRow, ClipboardRow, ImageEmbeddingRow, MediaItemRow, PomodoroItemRow,
    SessionRow, ShareRow, TrashedMessageRow, VideoPlayProgressRow,
};

fn clipboard(id: &str, created_at: &str) -> ClipboardRow {
    ClipboardRow {
        id: id.into(),
        text: format!("text-{id}"),
        hash: format!("hash-{id}"),
        source: String::new(),
        label: "label".into(),
        sensitive: id == "sensitive",
        created_at: created_at.into(),
    }
}

#[test]
fn clipboard_round_trips_and_orders_by_created_at() {
    let db = test_db("clipboard");
    db.clipboard_save(&clipboard("a", "2026-01-01T00:00:00Z"))
        .unwrap();
    db.clipboard_save(&clipboard("b", "2026-02-01T00:00:00Z"))
        .unwrap();

    let got = db.clipboard_get("a").unwrap().expect("row a");
    assert_eq!(got.text, "text-a");
    assert!(!got.sensitive);

    let latest = db.clipboard_latest().unwrap().expect("latest");
    assert_eq!(latest.id, "b");

    let by_hash = db
        .clipboard_latest_by_hash("hash-a")
        .unwrap()
        .expect("by hash");
    assert_eq!(by_hash.id, "a");

    // Saving the same id replaces rather than duplicating.
    let mut updated = clipboard("a", "2026-01-01T00:00:00Z");
    updated.text = "edited".into();
    db.clipboard_save(&updated).unwrap();
    assert_eq!(db.clipboard_get("a").unwrap().unwrap().text, "edited");
    assert_eq!(db.clipboard_count_rows().unwrap(), 2);
}

#[test]
fn clipboard_sensitive_flag_is_stored_as_a_boolean() {
    let db = test_db("clipboard_sensitive");
    db.clipboard_save(&clipboard("sensitive", "2026-01-01T00:00:00Z"))
        .unwrap();
    assert!(db.clipboard_get("sensitive").unwrap().unwrap().sensitive);
}

#[test]
fn clipboard_delete_by_ids_and_clear() {
    let db = test_db("clipboard_delete");
    for id in ["a", "b", "c"] {
        db.clipboard_save(&clipboard(id, "2026-01-01T00:00:00Z"))
            .unwrap();
    }
    assert_eq!(
        db.clipboard_delete_by_ids(&["a".into(), "c".into()])
            .unwrap(),
        2
    );
    assert_eq!(db.clipboard_count_rows().unwrap(), 1);
    // An empty id list must delete nothing rather than matching all rows.
    assert_eq!(db.clipboard_delete_by_ids(&[]).unwrap(), 0);
    assert_eq!(db.clipboard_clear().unwrap(), 1);
    assert_eq!(db.clipboard_count_rows().unwrap(), 0);
}

#[test]
fn session_save_get_and_touch() {
    let db = test_db("session");
    let row = SessionRow {
        client_id: "client-1".into(),
        name: "Browser".into(),
        r#type: "WEB".into(),
        client_ip: "192.168.1.5".into(),
        os_name: "macOS".into(),
        os_version: "15.0".into(),
        browser_name: "Chrome".into(),
        browser_version: "120".into(),
        token: "tok".into(),
        last_active_at: Some("2026-01-01T00:00:00Z".into()),
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-01T00:00:00Z".into(),
    };
    db.session_save(&row).unwrap();

    let got = db.session_get("client-1").unwrap().expect("session");
    assert_eq!(got.r#type, "WEB");
    assert_eq!(got.browser_name, "Chrome");

    db.session_touch(&[("client-1".into(), "2026-03-01T00:00:00Z".into())])
        .unwrap();
    let touched = db.session_get("client-1").unwrap().unwrap();
    assert_eq!(
        touched.last_active_at.as_deref(),
        Some("2026-03-01T00:00:00Z")
    );
    // Touching must not disturb the rest of the row.
    assert_eq!(touched.browser_version, "120");

    assert_eq!(db.session_list().unwrap().len(), 1);
    assert_eq!(db.session_delete("client-1").unwrap(), 1);
    assert!(db.session_get("client-1").unwrap().is_none());
}

#[test]
fn session_list_orders_by_last_active_desc() {
    let db = test_db("session_order");
    for (id, at) in [
        ("old", "2026-01-01T00:00:00Z"),
        ("new", "2026-05-01T00:00:00Z"),
        ("mid", "2026-03-01T00:00:00Z"),
    ] {
        db.session_save(&SessionRow {
            client_id: id.into(),
            name: id.into(),
            r#type: "WEB".into(),
            client_ip: "10.0.0.1".into(),
            os_name: String::new(),
            os_version: String::new(),
            browser_name: String::new(),
            browser_version: String::new(),
            token: String::new(),
            last_active_at: Some(at.into()),
            created_at: at.into(),
            updated_at: at.into(),
        })
        .unwrap();
    }
    let ids: Vec<String> = db
        .session_list()
        .unwrap()
        .into_iter()
        .map(|r| r.client_id)
        .collect();
    assert_eq!(ids, vec!["new", "mid", "old"]);
}

#[test]
fn share_round_trip() {
    let db = test_db("share");
    let row = ShareRow {
        id: "share-1".into(),
        name: "Photos".into(),
        password: String::new(),
        url_token: "url-token".into(),
        expires_at: Some("2027-01-01T00:00:00Z".into()),
        read_only: true,
        data: "[\"/a\"]".into(),
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-01T00:00:00Z".into(),
    };
    db.share_save(&row).unwrap();
    let got = db.share_get("share-1").unwrap().expect("share");
    assert!(got.read_only);
    assert_eq!(got.url_token, "url-token");
    assert_eq!(got.expires_at.as_deref(), Some("2027-01-01T00:00:00Z"));
    assert_eq!(db.share_list().unwrap().len(), 1);
    assert_eq!(db.share_delete("share-1").unwrap(), 1);
}

#[test]
fn pomodoro_queries() {
    let db = test_db("pomodoro");
    for (id, date, completed) in [
        ("p1", "2026-01-01", 3),
        ("p2", "2026-02-01", 4),
        ("p3", "2026-03-01", 5),
    ] {
        db.pomodoro_save(&PomodoroItemRow {
            id: id.into(),
            date: date.into(),
            completed_count: completed,
            total_work_seconds: 60,
            total_break_seconds: 30,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        })
        .unwrap();
    }
    let dates: Vec<String> = db
        .pomodoro_list()
        .unwrap()
        .into_iter()
        .map(|r| r.date)
        .collect();
    assert_eq!(dates, vec!["2026-03-01", "2026-02-01", "2026-01-01"]);

    let by_date = db.pomodoro_get_by_date("2026-02-01").unwrap().expect("row");
    assert_eq!(by_date.completed_count, 4);

    let recent = db.pomodoro_recent("2026-02-01", 10).unwrap();
    assert_eq!(recent.len(), 2);

    assert_eq!(db.pomodoro_total_completed().unwrap(), 12);
    assert_eq!(db.pomodoro_delete("p1").unwrap(), 1);
}

#[test]
fn media_item_and_video_progress_cache() {
    let db = test_db("media_cache");
    db.media_item_upsert(&MediaItemRow {
        media_type: "video".into(),
        media_id: "42".into(),
        duration_ms: 120_000,
        updated_at: "2026-01-01T00:00:00Z".into(),
    })
    .unwrap();
    let items = db.media_item_list().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].duration_ms, 120_000);

    // Re-upserting the same media id replaces instead of failing.
    db.media_item_upsert(&MediaItemRow {
        media_type: "video".into(),
        media_id: "42".into(),
        duration_ms: 240_000,
        updated_at: "2026-01-02T00:00:00Z".into(),
    })
    .unwrap();
    assert_eq!(db.media_item_list().unwrap().len(), 1);
    assert_eq!(db.media_item_list().unwrap()[0].duration_ms, 240_000);
    assert_eq!(db.media_item_delete("42").unwrap(), 1);

    db.video_progress_upsert(&VideoPlayProgressRow {
        media_id: "42".into(),
        position_ms: 5_000,
        updated_at: "2026-01-01T00:00:00Z".into(),
    })
    .unwrap();
    let progress = db.video_progress_get("42").unwrap().expect("progress");
    assert_eq!(progress.position_ms, 5_000);
    assert_eq!(
        db.video_progress_recent("2026-01-01T00:00:00Z")
            .unwrap()
            .len(),
        1
    );
    assert!(
        db.video_progress_recent("2027-01-01T00:00:00Z")
            .unwrap()
            .is_empty()
    );
    assert_eq!(db.video_progress_delete("42").unwrap(), 1);
}

#[test]
fn image_embedding_blob_survives_the_base64_boundary() {
    let db = test_db("embedding");
    // 0..=255 exercises every byte value through the JSON/base64 hop.
    let raw: Vec<u8> = (0u8..=255).collect();
    let encoded = base64::engine::general_purpose::STANDARD.encode(&raw);
    db.embedding_save(&ImageEmbeddingRow {
        id: "e1".into(),
        path: "/photos/a.jpg".into(),
        embedding_base64: encoded.clone(),
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-01T00:00:00Z".into(),
    })
    .unwrap();

    let rows = db.embedding_list().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].path, "/photos/a.jpg");
    assert_eq!(rows[0].embedding_base64, encoded);
    assert_eq!(db.embedding_ids().unwrap(), vec!["e1".to_string()]);
    assert_eq!(db.embedding_count().unwrap(), 1);

    assert_eq!(db.embedding_delete_by_ids(&["e1".into()]).unwrap(), 1);
    assert_eq!(db.embedding_count().unwrap(), 0);
}

#[test]
fn image_embedding_delete_all() {
    let db = test_db("embedding_all");
    for id in ["a", "b"] {
        db.embedding_save(&ImageEmbeddingRow {
            id: id.into(),
            path: format!("/{id}.jpg"),
            embedding_base64: String::new(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        })
        .unwrap();
    }
    assert_eq!(db.embedding_delete_all().unwrap(), 2);
    assert_eq!(db.embedding_count().unwrap(), 0);
}

#[test]
fn archived_conversation_round_trip() {
    let db = test_db("archived");
    db.archived_conversation_save(&ArchivedConversationRow {
        conversation_id: "conv-1".into(),
        conversation_date: "2026-01-01T00:00:00Z".into(),
    })
    .unwrap();
    let rows = db.archived_conversation_list().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].conversation_id, "conv-1");
    assert_eq!(db.archived_conversation_delete("conv-1").unwrap(), 1);
    assert!(db.archived_conversation_list().unwrap().is_empty());
}

#[test]
fn trashed_messages_bulk_insert_and_retention_cleanup() {
    let db = test_db("trashed");
    db.trashed_message_save_many(&[
        TrashedMessageRow {
            message_id: "1".into(),
            is_mms: false,
            trashed_at: "2026-01-01T00:00:00Z".into(),
        },
        TrashedMessageRow {
            message_id: "mms_2".into(),
            is_mms: true,
            trashed_at: "2026-06-01T00:00:00Z".into(),
        },
    ])
    .unwrap();

    let mut ids = db.trashed_message_ids().unwrap();
    ids.sort();
    assert_eq!(ids, vec!["1".to_string(), "mms_2".to_string()]);

    assert_eq!(
        db.trashed_message_delete_older_than("2026-03-01T00:00:00Z")
            .unwrap(),
        1
    );
    assert_eq!(db.trashed_message_ids().unwrap(), vec!["mms_2".to_string()]);
    assert_eq!(
        db.trashed_message_delete_by_ids(&["mms_2".into()]).unwrap(),
        1
    );
    assert!(db.trashed_message_ids().unwrap().is_empty());
}

/// The FFI boundary needs both directions: Kotlin serializes a row on write
/// and deserializes the same shape on read.
#[test]
fn rows_round_trip_through_serde() {
    let original = ClipboardRow {
        id: "c1".into(),
        text: "hello".into(),
        hash: "h1".into(),
        source: "peer".into(),
        label: "label".into(),
        sensitive: true,
        created_at: "2026-01-01T00:00:00Z".into(),
    };
    let json = serde_json::to_string(&original).unwrap();
    let back: ClipboardRow = serde_json::from_str(&json).unwrap();
    assert_eq!(back.id, original.id);
    assert_eq!(back.text, original.text);
    assert_eq!(back.hash, original.hash);
    assert_eq!(back.source, original.source);
    assert_eq!(back.label, original.label);
    assert_eq!(back.sensitive, original.sensitive);
    assert_eq!(back.created_at, original.created_at);
}

/// Column names stay snake_case so the Kotlin data classes can map them
/// without a translation layer.
#[test]
fn serialized_field_names_are_snake_case() {
    let row = SessionRow {
        client_id: "c".into(),
        name: "n".into(),
        r#type: "WEB".into(),
        client_ip: "1.2.3.4".into(),
        os_name: String::new(),
        os_version: String::new(),
        browser_name: String::new(),
        browser_version: String::new(),
        token: "t".into(),
        last_active_at: None,
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-01T00:00:00Z".into(),
    };
    let value: serde_json::Value = serde_json::to_value(&row).unwrap();
    let object = value.as_object().unwrap();
    for key in [
        "client_id",
        "client_ip",
        "os_name",
        "os_version",
        "browser_name",
        "browser_version",
        "last_active_at",
        "created_at",
        "updated_at",
    ] {
        assert!(object.contains_key(key), "missing snake_case key {key}");
    }
    // The Rust keyword field is exposed under its plain name, not `r#type`.
    assert_eq!(object["type"], "WEB");
}

/// The queue-source enum serializes as the plain-app enum names.
#[test]
fn queue_source_kind_serializes_as_upper_snake() {
    use crate::db::models::audio_queue::QueueSourceKind;
    assert_eq!(
        serde_json::to_string(&QueueSourceKind::None).unwrap(),
        "\"NONE\""
    );
    assert_eq!(
        serde_json::to_string(&QueueSourceKind::Playlist).unwrap(),
        "\"PLAYLIST\""
    );
    assert_eq!(
        serde_json::to_string(&QueueSourceKind::Library).unwrap(),
        "\"LIBRARY\""
    );
    let back: QueueSourceKind = serde_json::from_str("\"LIBRARY\"").unwrap();
    assert_eq!(back, QueueSourceKind::Library);
}
