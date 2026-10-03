use super::*;
use crate::{
    chat::app_file_store::import_bytes,
    db::{
        DChat,
        chat_store::{SaveMode, messages},
    },
};

#[test]
fn deletion_releases_exact_references_and_preserves_other_conversations() {
    let root = std::env::temp_dir().join(format!(
        "plain-chat-delete-{}",
        crate::utils::short_uuid::short_uuid()
    ));
    fs::create_dir_all(&root).unwrap();
    let db = Db::open(&root.join("plain.db")).unwrap();
    let file = import_bytes(&db, &root, b"owned fixture", "text/plain").unwrap();
    import_bytes(&db, &root, b"owned fixture", "text/plain").unwrap();
    import_bytes(&db, &root, b"owned fixture", "text/plain").unwrap();
    let content = serde_json::json!({"type":"FILES","value":{"items":[{"uri":format!("fid:{}",file.fid_suffix)},{"uri":"fsid:foreign"}]}}).to_string();
    let mut direct = DChat::new("me", "peer", "", &content);
    direct.id = "direct".into();
    let mut group = DChat::new("peer", "me", "group", &content);
    group.id = "group-message".into();
    messages::save(&db, &[direct, group], SaveMode::Insert).unwrap();
    assert_eq!(
        delete(
            &db,
            &root,
            Selection::Ids(&["direct".into(), "direct".into(), "absent".into()])
        )
        .unwrap(),
        1
    );
    assert_eq!(db.app_file_get(&file.id).unwrap().unwrap().ref_count, 2);
    assert!(root.join(&file.real_path).exists());
    assert_eq!(delete(&db, &root, Selection::Peer("peer")).unwrap(), 0);
    assert!(messages::get(&db, "group-message").unwrap().is_some());
    assert_eq!(delete(&db, &root, Selection::Channel("group")).unwrap(), 1);
    assert_eq!(db.app_file_get(&file.id).unwrap().unwrap().ref_count, 1);
    assert!(root.join(&file.real_path).exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn failed_sql_restores_attachment_and_message_then_retry_deletes_once() {
    let root = std::env::temp_dir().join(format!(
        "plain-chat-rollback-{}",
        crate::utils::short_uuid::short_uuid()
    ));
    fs::create_dir_all(&root).unwrap();
    let db = Db::open(&root.join("plain.db")).unwrap();
    let file = import_bytes(&db, &root, b"rollback fixture", "image/png").unwrap();
    let content = serde_json::json!({"type":"IMAGES","value":{"items":[{"uri":format!("fid:{}",file.fid_suffix)}]}}).to_string();
    let mut chat = DChat::new("me", "peer", "", &content);
    chat.id = "message".into();
    messages::save(&db, &[chat], SaveMode::Insert).unwrap();
    db.with_conn(|c| c.execute_batch("CREATE TRIGGER reject_delete BEFORE DELETE ON chats BEGIN SELECT RAISE(ABORT,'test rollback'); END;")).unwrap();
    assert!(delete(&db, &root, Selection::Peer("peer")).is_err());
    assert!(messages::get(&db, "message").unwrap().is_some());
    assert_eq!(db.app_file_get(&file.id).unwrap().unwrap().ref_count, 1);
    assert_eq!(
        fs::read(root.join(&file.real_path)).unwrap(),
        b"rollback fixture"
    );
    db.with_conn(|c| c.execute_batch("DROP TRIGGER reject_delete"))
        .unwrap();
    assert_eq!(delete(&db, &root, Selection::Peer("peer")).unwrap(), 1);
    assert!(db.app_file_get(&file.id).unwrap().is_none());
    assert!(!root.join(&file.real_path).exists());
    assert_eq!(
        delete(&db, &root, Selection::Ids(&["message".into()])).unwrap(),
        0
    );
    fs::remove_dir_all(root).unwrap();
}
