use super::*;

fn metadata(path: &str) -> MediaFileMetadata {
    MediaFileMetadata {
        data_type: DataType::Image,
        path: path.into(),
        title: "Photo".into(),
        artist: String::new(),
        bucket_id: "photos".into(),
        size_bytes: 10,
        duration_ms: 0,
        album_file_id: String::new(),
        width: 640,
        height: 480,
        rotation: 0,
        taken_at: None,
        created_at: "2026-10-12T00:00:00Z".into(),
        updated_at: "2026-10-12T00:00:00Z".into(),
    }
}

#[test]
fn rescanning_preserves_identity_created_time_and_business_flags() {
    let directory = tempfile::tempdir().unwrap();
    let db = Db::open(&directory.path().join("plain.db")).unwrap();
    let mut file = metadata("/photos/one.jpg");
    let original = db.record_media_files(&[file.clone()]).unwrap().remove(0);
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE media_files SET is_favorite=1,is_trashed=1,original_path=path WHERE id=?1",
            [&original.id],
        )
    })
    .unwrap();
    file.title = "Updated".into();
    file.size_bytes = 20;
    file.created_at = "2026-10-13T00:00:00Z".into();
    file.updated_at = file.created_at.clone();
    let updated = db.record_media_files(&[file]).unwrap().remove(0);
    assert_eq!(updated.id, original.id);
    assert_eq!(updated.created_at, original.created_at);
    assert_eq!(updated.title, "Updated");
    assert_eq!(updated.size_bytes, 20);
    assert!(updated.is_favorite && updated.is_trashed);
    assert_eq!(updated.original_path.as_deref(), Some("/photos/one.jpg"));
    assert_eq!(db.media_file_count(DataType::Image).unwrap(), 0);
    assert!(
        db.media_file_list(DataType::Image, 10, 0)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn batches_rollback_on_invalid_metadata_and_domains_are_separate() {
    let directory = tempfile::tempdir().unwrap();
    let db = Db::open(&directory.path().join("plain.db")).unwrap();
    let first = metadata("/photos/one.jpg");
    let mut invalid = metadata("/photos/two.jpg");
    invalid.size_bytes = -1;
    assert!(db.record_media_files(&[first.clone(), invalid]).is_err());
    assert_eq!(db.media_file_count(DataType::Image).unwrap(), 0);
    let image = db.record_media_files(&[first.clone()]).unwrap().remove(0);
    let mut audio = first;
    audio.data_type = DataType::Audio;
    let audio = db.record_media_files(&[audio]).unwrap().remove(0);
    assert_ne!(image.id, audio.id);
    assert_eq!(db.media_file_count(DataType::Image).unwrap(), 1);
    assert_eq!(db.media_file_count(DataType::Audio).unwrap(), 1);
    assert!(
        db.media_file_by_path(DataType::Image, "/absent")
            .unwrap()
            .is_none()
    );
    assert!(db.media_file_count(DataType::Note).is_err());
}

#[test]
fn unavailable_probe_fields_preserve_current_metadata_but_successful_empty_values_clear_it() {
    let directory = tempfile::tempdir().unwrap();
    let db = Db::open(&directory.path().join("plain.db")).unwrap();
    let mut known = metadata("/photos/one.jpg");
    known.artist = "Artist".into();
    known.duration_ms = 12_000;
    known.album_file_id = "album".into();
    known.rotation = 90;
    known.taken_at = Some("2026-10-11T00:00:00Z".into());
    let original = db.record_media_files(&[known.clone()]).unwrap().remove(0);
    let mut unknown = metadata(&known.path);
    unknown.title = "Filename fallback".into();
    unknown.artist.clear();
    unknown.duration_ms = 0;
    unknown.album_file_id.clear();
    unknown.width = 0;
    unknown.height = 0;
    unknown.rotation = 0;
    unknown.taken_at = None;
    unknown.size_bytes = 20;
    let preserved = db
        .record_media_file_reads(&[MediaFileMetadataRead {
            metadata: unknown.clone(),
            available: Default::default(),
        }])
        .unwrap()
        .remove(0);
    assert_eq!(preserved.id, original.id);
    assert_eq!(preserved.title, known.title);
    assert_eq!(preserved.artist, known.artist);
    assert_eq!(preserved.duration_ms, known.duration_ms);
    assert_eq!(preserved.album_file_id, known.album_file_id);
    assert_eq!(
        (preserved.width, preserved.height, preserved.rotation),
        (known.width, known.height, known.rotation)
    );
    assert_eq!(preserved.taken_at, known.taken_at);
    assert_eq!(preserved.size_bytes, 20);
    let cleared = db
        .record_media_file_reads(&[MediaFileMetadataRead {
            metadata: unknown,
            available: MediaFileMetadataAvailability::ALL,
        }])
        .unwrap()
        .remove(0);
    assert_eq!(cleared.title, "Filename fallback");
    assert!(cleared.artist.is_empty());
    assert_eq!(cleared.duration_ms, 0);
    assert!(cleared.album_file_id.is_empty());
    assert_eq!((cleared.width, cleared.height, cleared.rotation), (0, 0, 0));
    assert_eq!(cleared.taken_at, None);
    assert_eq!(cleared.id, original.id);
}

#[test]
fn availability_aware_batches_are_atomic_and_new_unknown_files_are_not_hidden() {
    let directory = tempfile::tempdir().unwrap();
    let db = Db::open(&directory.path().join("plain.db")).unwrap();
    let original = db
        .record_media_files(&[metadata("/photos/one.jpg")])
        .unwrap()
        .remove(0);
    let mut updated = metadata("/photos/one.jpg");
    updated.title = "Replacement".into();
    let mut invalid = metadata("/photos/two.jpg");
    invalid.size_bytes = -1;
    assert!(
        db.record_media_file_reads(&[
            MediaFileMetadataRead {
                metadata: updated,
                available: MediaFileMetadataAvailability::ALL
            },
            MediaFileMetadataRead {
                metadata: invalid,
                available: Default::default()
            },
        ])
        .is_err()
    );
    assert_eq!(
        db.media_file_by_path(DataType::Image, "/photos/one.jpg")
            .unwrap()
            .unwrap(),
        original
    );
    let inserted = db
        .record_media_file_reads(&[MediaFileMetadataRead {
            metadata: metadata("/photos/new.heic"),
            available: Default::default(),
        }])
        .unwrap()
        .remove(0);
    assert_eq!(inserted.path, "/photos/new.heic");
    assert_eq!(db.media_file_count(DataType::Image).unwrap(), 2);
}
