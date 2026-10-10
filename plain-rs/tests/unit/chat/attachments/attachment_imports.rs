use super::*;
fn fixture() -> (PathBuf, Db, Imports) {
    let dir = std::env::temp_dir().join(format!(
        "plain-attachment-test-{}",
        crate::utils::short_uuid::short_uuid()
    ));
    let db = Db::open(&dir.join("plain.db")).unwrap();
    let mut chat = crate::db::DChat::new(
        "me",
        "peer",
        "",
        &serde_json::json!({"type":"FILES","extra":"preserved","value":{"items":[
            {"id":"a","uri":"fsid:remote-a","fileName":"../../a.txt","size":3},
            {"id":"b","uri":"fsid:remote-b","fileName":"b.txt","size":3}
        ]}})
        .to_string(),
    );
    chat.id = "message".into();
    db.insert_chat(&chat);
    (dir, db, Imports::default())
}
fn begin(imports: &Imports, db: &Db, dir: &Path, id: &str) -> Ticket {
    imports
        .begin(db, dir, "message", id, &format!("fsid:remote-{id}"))
        .unwrap()
}
#[test]
fn concurrent_attachments_preserve_current_content_and_count_references() {
    let (dir, db, imports) = fixture();
    let a = begin(&imports, &db, &dir, "a");
    let b = begin(&imports, &db, &dir, "b");
    assert_eq!(
        Path::new(&a.path).parent().unwrap(),
        dir.canonicalize().unwrap().join("attachment-transfers")
    );
    fs::write(&a.path, b"abc").unwrap();
    fs::write(&b.path, b"abc").unwrap();
    let first = imports.finish(&db, &dir, &a.token).unwrap();
    db.with_conn(|c| {
        c.execute(
            "UPDATE chats SET content=json_set(content,'$.extra','changed') WHERE id='message'",
            [],
        )
    })
    .unwrap();
    let second = imports.finish(&db, &dir, &b.token).unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(db.app_file_get(&first.id).unwrap().unwrap().ref_count, 2);
    let current: Value = serde_json::from_str(&second.chat.unwrap().content).unwrap();
    assert_eq!(current["extra"], "changed");
    assert!(
        current["value"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["uri"] == format!("fid:{}", first.fid_suffix))
    );
    assert!(imports.finish(&db, &dir, &a.token).is_err());
    assert!(!Path::new(&a.path).exists());
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn sql_failure_rolls_back_message_and_new_physical_file() {
    let (dir, db, imports) = fixture();
    let a = begin(&imports, &db, &dir, "a");
    fs::write(&a.path, b"abc").unwrap();
    db.with_conn(|c|c.execute_batch("CREATE TRIGGER reject_file BEFORE INSERT ON app_files BEGIN SELECT RAISE(ABORT,'fixture'); END;")).unwrap();
    assert!(imports.finish(&db, &dir, &a.token).is_err());
    let chat = chats::get(&db, "message").unwrap().unwrap();
    assert!(chat.content.contains("fsid:remote-a"));
    assert_eq!(
        db.with_conn(|c| c.query_row("SELECT count(*) FROM app_files", [], |r| r.get::<_, i64>(0)))
            .unwrap(),
        0
    );
    use sha2::{Digest, Sha256};
    let hash = crate::utils::hex::bytes_to_hex(&Sha256::digest(b"abc"));
    assert!(!app_file_store::dest_path(&dir, &hash, "txt").exists());
    assert!(!Path::new(&a.path).exists());
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn changed_deleted_and_incomplete_attachments_cannot_commit() {
    for mode in ["changed", "deleted", "short", "long"] {
        let (dir, db, imports) = fixture();
        let a = begin(&imports, &db, &dir, "a");
        fs::write(
            &a.path,
            if mode == "short" {
                b"ab".as_slice()
            } else if mode == "long" {
                b"abcd".as_slice()
            } else {
                b"abc".as_slice()
            },
        )
        .unwrap();
        if mode == "changed" {
            db.with_conn(|c|c.execute("UPDATE chats SET content=json_set(content,'$.value.items[0].uri','fsid:new') WHERE id='message'",[])).unwrap();
        }
        if mode == "deleted" {
            db.with_conn(|c| c.execute("DELETE FROM chats WHERE id='message'", []))
                .unwrap();
        }
        assert!(imports.finish(&db, &dir, &a.token).is_err(), "{mode}");
        assert_eq!(
            db.with_conn(
                |c| c.query_row("SELECT count(*) FROM app_files", [], |r| r.get::<_, i64>(0))
            )
            .unwrap(),
            0
        );
        fs::remove_dir_all(dir).unwrap();
    }
}
#[test]
fn old_abort_and_receipt_do_not_touch_replacement_transfer() {
    let (dir, db, imports) = fixture();
    let old = begin(&imports, &db, &dir, "a");
    assert!(imports.abort(&old.token).unwrap());
    let new = begin(&imports, &db, &dir, "a");
    assert!(!imports.abort(&old.token).unwrap());
    assert!(imports.finish(&db, &dir, &old.token).is_err());
    assert!(Path::new(&new.path).exists());
    fs::write(&new.path, b"abc").unwrap();
    imports.finish(&db, &dir, &new.token).unwrap();
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn dedup_failure_does_not_retain_existing_file() {
    let (dir, db, imports) = fixture();
    let a = begin(&imports, &db, &dir, "a");
    fs::write(&a.path, b"abc").unwrap();
    let first = imports.finish(&db, &dir, &a.token).unwrap();
    let b = begin(&imports, &db, &dir, "b");
    fs::write(&b.path, b"abc").unwrap();
    db.with_conn(|c|c.execute_batch("CREATE TRIGGER reject_ref BEFORE UPDATE ON app_files BEGIN SELECT RAISE(ABORT,'fixture'); END;")).unwrap();
    assert!(imports.finish(&db, &dir, &b.token).is_err());
    assert_eq!(db.app_file_get(&first.id).unwrap().unwrap().ref_count, 1);
    assert!(first.real_path.exists());
    assert!(
        chats::get(&db, "message")
            .unwrap()
            .unwrap()
            .content
            .contains("fsid:remote-b")
    );
    fs::remove_dir_all(dir).unwrap();
}
#[cfg(unix)]
#[test]
fn transfer_directory_and_source_symlinks_are_rejected() {
    let (dir, db, imports) = fixture();
    let external = dir.join("external");
    fs::create_dir(&external).unwrap();
    std::os::unix::fs::symlink(&external, dir.join("attachment-transfers")).unwrap();
    assert!(
        imports
            .begin(&db, &dir, "message", "a", "fsid:remote-a")
            .is_err()
    );
    fs::remove_file(dir.join("attachment-transfers")).unwrap();
    let a = begin(&imports, &db, &dir, "a");
    fs::remove_file(&a.path).unwrap();
    let source = external.join("source");
    fs::write(&source, b"abc").unwrap();
    std::os::unix::fs::symlink(&source, &a.path).unwrap();
    assert!(imports.finish(&db, &dir, &a.token).is_err());
    assert!(source.exists());
    fs::remove_dir_all(dir).unwrap();
}
