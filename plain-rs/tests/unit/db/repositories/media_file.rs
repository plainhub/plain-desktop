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
