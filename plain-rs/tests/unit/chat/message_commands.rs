use super::*;
use crate::chat::app_file_store::{
    self,
    chat_deletion::{self, Selection},
};
fn fixture() -> (tempfile::TempDir, Db, app_file_store::ImportResult) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("plain.db")).unwrap();
    let file =
        app_file_store::import_bytes(&db, dir.path(), b"synthetic owned attachment", "text/plain")
            .unwrap();
    (dir, db, file)
}
fn item(id: &str, file: &app_file_store::ImportResult) -> Value {
    json!({"id":id,"uri":format!("fid:{}",file.fid_suffix),"fileName":"fixture.txt","size":26,"loading":false})
}
fn refs(db: &Db, id: &str) -> i64 {
    db.with_conn(|c| {
        c.query_row("SELECT ref_count FROM app_files WHERE id=?1", [id], |r| {
            r.get(0)
        })
    })
    .unwrap()
}
#[test]
fn one_import_shared_across_targets_keeps_physical_content_until_last_owner() {
    let (dir, db, file) = fixture();
    let first = create_files(&db, "peer:local", vec![item("file", &file)], false).unwrap();
    assert_eq!(refs(&db, &file.id), 1);
    let second = create_files(&db, "peer:other", vec![item("file", &file)], false).unwrap();
    assert_eq!(refs(&db, &file.id), 2);
    chat_deletion::delete(&db, dir.path(), Selection::Ids(&[first.id])).unwrap();
    assert_eq!(refs(&db, &file.id), 1);
    assert!(file.real_path.exists());
    chat_deletion::delete(&db, dir.path(), Selection::Ids(&[second.id])).unwrap();
    assert!(!file.real_path.exists());
}
#[test]
fn a_fresh_deduplicated_import_is_consumed_once_without_double_increment() {
    let (dir, db, file) = fixture();
    create_files(&db, "local", vec![item("first", &file)], true).unwrap();
    let imported =
        app_file_store::import_bytes(&db, dir.path(), b"synthetic owned attachment", "text/plain")
            .unwrap();
    assert_eq!(refs(&db, &file.id), 2);
    create_files(&db, "peer:other", vec![item("second", &imported)], true).unwrap();
    assert_eq!(refs(&db, &file.id), 2);
}
#[test]
fn file_content_status_and_reference_release_are_one_transaction_with_disk_rollback() {
    let (dir, db, file) = fixture();
    let row = create_files(&db, "peer:other", vec![item("file", &file)], false).unwrap();
    let next =
        app_file_store::import_bytes(&db, dir.path(), b"synthetic replacement", "text/plain")
            .unwrap();
    db.with_conn(|c|c.execute_batch("CREATE TRIGGER reject_attachment_edit BEFORE UPDATE OF content ON chats BEGIN SELECT RAISE(FAIL,'synthetic SQL refusal'); END")).unwrap();
    assert!(replace_files(&db, dir.path(), &row.id, vec![item("next", &next)]).is_err());
    let unchanged = crate::db::chat_store::messages::get(&db, &row.id)
        .unwrap()
        .unwrap();
    assert_eq!(unchanged.content, row.content);
    assert!(file.real_path.exists());
    assert_eq!(refs(&db, &file.id), 1);
    db.with_conn(|c| c.execute_batch("DROP TRIGGER reject_attachment_edit"))
        .unwrap();
    let saved = replace_files(&db, dir.path(), &row.id, vec![item("next", &next)])
        .unwrap()
        .unwrap();
    assert_eq!(saved.status, ChatStatus::Pending);
    assert_eq!(
        serde_json::from_str::<Value>(&saved.content).unwrap()["value"]["items"][0]["id"],
        "next"
    );
    assert!(!file.real_path.exists());
    assert_eq!(refs(&db, &next.id), 1);
    let saved = replace_files(&db, dir.path(), &row.id, vec![])
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&saved.content).unwrap()["value"]["items"],
        json!([])
    );
    assert!(!next.real_path.exists());
}
#[test]
fn unknown_owned_files_and_duplicate_attachment_ids_cannot_partially_create_messages() {
    let (_dir, db, file) = fixture();
    assert!(
        create_files(
            &db,
            "local",
            vec![item("same", &file), item("same", &file)],
            false
        )
        .is_err()
    );
    assert_eq!(refs(&db, &file.id), 1);
    assert!(
        create_files(
            &db,
            "local",
            vec![json!({"id":"file","uri":"fid:unknown.txt","size":3})],
            false
        )
        .is_err()
    );
    assert!(
        crate::db::chat_store::messages::all(&db)
            .unwrap()
            .is_empty()
    );
    assert!(target("channel:").is_err());
}

#[test]
fn batch_binding_is_atomic_and_uses_actual_imported_size() {
    let (dir, db, file) = fixture();
    let placeholder = json!({"id":"file","uri":"content://synthetic","size":0});
    let a = create_files(&db, "local", vec![placeholder.clone()], false).unwrap();
    let c = create_files(&db, "peer:other", vec![placeholder], false).unwrap();
    db.with_conn(|conn|conn.execute_batch(&format!("CREATE TRIGGER reject_second BEFORE UPDATE OF content ON chats WHEN OLD.id='{}' BEGIN SELECT RAISE(FAIL,'synthetic refusal'); END",c.id))).unwrap();
    assert!(
        replace_many(
            &db,
            dir.path(),
            &[a.id.clone(), c.id.clone()],
            vec![item("file", &file)]
        )
        .is_err()
    );
    assert_eq!(
        crate::db::chat_store::messages::get(&db, &a.id)
            .unwrap()
            .unwrap()
            .content,
        a.content
    );
    assert_eq!(refs(&db, &file.id), 1);
    db.with_conn(|conn| conn.execute_batch("DROP TRIGGER reject_second"))
        .unwrap();
    let saved = replace_many(&db, dir.path(), &[a.id, c.id], vec![item("file", &file)]).unwrap();
    assert_eq!(refs(&db, &file.id), 2);
    assert_eq!(saved[0].as_ref().unwrap().status, ChatStatus::Sent);
    assert_eq!(saved[1].as_ref().unwrap().status, ChatStatus::Pending);
    let actual = std::fs::metadata(&file.real_path).unwrap().len();
    assert_eq!(
        serde_json::from_str::<Value>(&saved[0].as_ref().unwrap().content).unwrap()["value"]["items"]
            [0]["size"],
        json!(actual)
    );
}
