use super::*;
use crate::chat::app_file_store::{chat_deletion, import_preview_image};
use serde_json::json;
fn fixture() -> (tempfile::TempDir, Db) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("plain.db")).unwrap();
    let mut row=DChat::new("me","local","",&json!({"type":"TEXT","metadata":"preserve","value":{"text":"https://a.example https://b.example","linkPreviews":[]}}).to_string());
    row.id = "message".into();
    db.insert_chat(&row);
    (dir, db)
}
fn image(db: &Db, dir: &Path, url: &str) -> crate::chat::app_file_store::ImportResult {
    import_preview_image(
        db,
        dir,
        b"unique-preview-image",
        "image/png",
        "message",
        "https://a.example https://b.example",
        &json!({"url":url,"title":"fixture"}),
    )
    .unwrap()
}
#[test]
fn preview_import_merges_current_content_and_edit_releases_per_occurrence() {
    let (dir, db) = fixture();
    let first = image(&db, dir.path(), "https://a.example");
    db.with_conn(|c| {
        c.execute(
            "UPDATE chats SET content=json_set(content,'$.metadata','changed') WHERE id='message'",
            [],
        )
    })
    .unwrap();
    image(&db, dir.path(), "https://b.example");
    assert_eq!(db.app_file_get(&first.id).unwrap().unwrap().ref_count, 2);
    let edited = edit(&db, dir.path(), "message", "https://b.example").unwrap();
    assert!(edited.changed);
    let content: Value = serde_json::from_str(&edited.chat.content).unwrap();
    assert_eq!(content["metadata"], "changed");
    assert_eq!(
        content["value"]["linkPreviews"].as_array().unwrap().len(),
        1
    );
    assert_eq!(db.app_file_get(&first.id).unwrap().unwrap().ref_count, 1);
    assert!(first.real_path.exists());
    assert!(
        !edit(&db, dir.path(), "message", "https://b.example")
            .unwrap()
            .changed
    );
    chat_deletion::delete(
        &db,
        dir.path(),
        chat_deletion::Selection::Ids(&["message".into()]),
    )
    .unwrap();
    assert!(db.app_file_get(&first.id).unwrap().is_none());
    assert!(!first.real_path.exists());
}
#[test]
fn stale_duplicate_and_deleted_preview_images_never_retain_or_leave_files() {
    let (dir, db) = fixture();
    let first = image(&db, dir.path(), "https://a.example");
    assert!(
        import_preview_image(
            &db,
            dir.path(),
            b"unique-preview-image",
            "image/png",
            "message",
            "https://a.example https://b.example",
            &json!({"url":"https://a.example"})
        )
        .is_err()
    );
    assert_eq!(db.app_file_get(&first.id).unwrap().unwrap().ref_count, 1);
    edit(&db, dir.path(), "message", "new text").unwrap();
    assert!(!first.real_path.exists());
    assert!(
        import_preview_image(
            &db,
            dir.path(),
            b"new-image",
            "image/png",
            "message",
            "https://a.example https://b.example",
            &json!({"url":"https://b.example"})
        )
        .is_err()
    );
    db.with_conn(|c| c.execute("DELETE FROM chats WHERE id='message'", []))
        .unwrap();
    assert!(
        import_preview_image(
            &db,
            dir.path(),
            b"new-image",
            "image/png",
            "message",
            "new text",
            &json!({"url":"https://b.example"})
        )
        .is_err()
    );
    assert_eq!(
        db.with_conn(|c| c.query_row("SELECT count(*) FROM app_files", [], |r| r.get::<_, i64>(0)))
            .unwrap(),
        0
    );
    assert!(
        append(
            &db,
            "message",
            "new text",
            &json!({"url":"https://b.example"})
        )
        .unwrap()
        .is_none()
    );
}
#[test]
fn failed_edit_restores_quarantined_image_and_sql_state() {
    let (dir, db) = fixture();
    let first = image(&db, dir.path(), "https://a.example");
    db.with_conn(|c|c.execute_batch("CREATE TRIGGER reject BEFORE UPDATE ON chats BEGIN SELECT RAISE(ABORT,'fixture'); END;")).unwrap();
    assert!(edit(&db, dir.path(), "message", "new text").is_err());
    assert!(first.real_path.exists());
    assert_eq!(db.app_file_get(&first.id).unwrap().unwrap().ref_count, 1);
    let content: Value = serde_json::from_str(
        &crate::db::chat_store::messages::get(&db, "message")
            .unwrap()
            .unwrap()
            .content,
    )
    .unwrap();
    assert_eq!(
        content["value"]["text"],
        "https://a.example https://b.example"
    );
    assert_eq!(
        content["value"]["linkPreviews"].as_array().unwrap().len(),
        1
    );
}
#[test]
fn failed_import_rolls_back_preview_and_physically_installed_image() {
    let (dir, db) = fixture();
    db.with_conn(|c|c.execute_batch("CREATE TRIGGER reject BEFORE INSERT ON app_files BEGIN SELECT RAISE(ABORT,'fixture'); END;")).unwrap();
    assert!(
        import_preview_image(
            &db,
            dir.path(),
            b"new-image",
            "image/png",
            "message",
            "https://a.example https://b.example",
            &json!({"url":"https://a.example"})
        )
        .is_err()
    );
    assert_eq!(
        db.with_conn(|c| c.query_row("SELECT count(*) FROM app_files", [], |r| r.get::<_, i64>(0)))
            .unwrap(),
        0
    );
    let row = crate::db::chat_store::messages::get(&db, "message")
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&row.content).unwrap()["value"]["linkPreviews"],
        json!([])
    );
    use sha2::{Digest, Sha256};
    let hash = crate::utils::hex::bytes_to_hex(&Sha256::digest(b"new-image"));
    assert!(!crate::chat::app_file_store::dest_path(dir.path(), &hash, "png").exists());
}
