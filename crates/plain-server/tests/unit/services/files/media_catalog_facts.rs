use super::*;

fn row(data_type: DataType) -> MediaFileRow {
    MediaFileRow {
        id: "id".into(),
        data_type: data_type.kind(),
        path: "/media/one".into(),
        title: "One".into(),
        artist: "Artist".into(),
        bucket_id: "bucket".into(),
        size_bytes: 30,
        duration_ms: 10,
        album_file_id: "album".into(),
        width: 4,
        height: 3,
        rotation: 90,
        taken_at: Some("2026-10-12T00:00:00Z".into()),
        created_at: "2026-10-12T00:00:00Z".into(),
        updated_at: "2026-10-13T00:00:00Z".into(),
        is_favorite: true,
        is_trashed: false,
        original_path: Some("/trash/one".into()),
    }
}

const LIBRARY_TYPES: [DataType; 4] = [
    DataType::Audio,
    DataType::Video,
    DataType::Image,
    DataType::Doc,
];

#[test]
fn every_row_carries_the_shared_library_fields() {
    let facts = row_facts(&row(DataType::Video)).unwrap();
    assert_eq!(facts["id"], json!("id"));
    assert_eq!(facts["title"], json!("One"));
    assert_eq!(facts["path"], json!("/media/one"));
    assert_eq!(facts["size"], json!(30));
    assert_eq!(facts["bucketId"], json!("bucket"));
    assert_eq!(facts["createdAt"], json!("2026-10-12T00:00:00Z"));
    assert_eq!(facts["updatedAt"], json!("2026-10-13T00:00:00Z"));
    assert_eq!(facts["isFavorite"], json!(true));
}

#[test]
fn audio_facts_carry_the_audio_only_fields() {
    let facts = row_facts(&row(DataType::Audio)).unwrap();
    assert_eq!(facts["artist"], json!("Artist"));
    assert_eq!(facts["durationMs"], json!(10));
    assert_eq!(facts["albumFileId"], json!("album"));
    for key in ["takenAt", "width", "height", "rotation"] {
        assert!(facts.get(key).is_none());
    }
}

#[test]
fn video_image_and_doc_share_the_dimension_fields() {
    for data_type in [DataType::Video, DataType::Image, DataType::Doc] {
        let facts = row_facts(&row(data_type)).unwrap();
        assert_eq!(facts["durationMs"], json!(10));
        assert_eq!(facts["takenAt"], json!("2026-10-12T00:00:00Z"));
        assert_eq!(facts["width"], json!(4));
        assert_eq!(facts["height"], json!(3));
        assert_eq!(facts["rotation"], json!(90));
        for key in ["artist", "albumFileId"] {
            assert!(facts.get(key).is_none());
        }
    }
}

#[test]
fn missing_taken_at_projects_as_json_null() {
    let mut media = row(DataType::Image);
    media.taken_at = None;
    let facts = row_facts(&media).unwrap();
    assert!(facts.as_object().unwrap().contains_key("takenAt"));
    assert!(facts["takenAt"].is_null());
}

#[test]
fn sixty_four_bit_values_stay_integers() {
    let mut media = row(DataType::Audio);
    media.size_bytes = 9_007_199_254_740_993;
    media.duration_ms = i64::MAX;
    let facts = row_facts(&media).unwrap();
    assert_eq!(facts["size"].as_i64(), Some(9_007_199_254_740_993));
    assert_eq!(facts["durationMs"].as_i64(), Some(i64::MAX));
    assert_eq!(
        serde_json::to_string(&facts["size"]).unwrap(),
        "9007199254740993"
    );
    assert_eq!(
        serde_json::to_string(&facts["durationMs"]).unwrap(),
        "9223372036854775807"
    );
}

#[test]
fn unsupported_data_types_are_rejected() {
    assert!(row_facts(&row(DataType::Note)).is_err());
    let mut media = row(DataType::Audio);
    media.data_type = 999;
    assert!(row_facts(&media).is_err());
}

#[test]
fn trashed_rows_project_the_same_shape_as_live_rows() {
    for data_type in LIBRARY_TYPES {
        let live = row_facts(&row(data_type)).unwrap();
        let mut trashed = row(data_type);
        trashed.is_trashed = true;
        assert_eq!(row_facts(&trashed).unwrap(), live);
    }
}

#[test]
fn facts_never_expose_private_row_columns() {
    for data_type in LIBRARY_TYPES {
        let facts = row_facts(&row(data_type)).unwrap();
        for key in ["isTrashed", "originalPath", "dataType", "data_type"] {
            assert!(facts.get(key).is_none());
        }
    }
}
