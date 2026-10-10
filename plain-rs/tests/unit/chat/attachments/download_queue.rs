use super::*;
fn fixture(count: usize) -> (tempfile::TempDir, Db, Queue) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("plain.db")).unwrap();
    let peer = DPeer::new(
        "peer",
        "fixture",
        "127.0.0.1",
        443,
        crate::chat::enums::DeviceType::Phone,
    );
    peers::save(&db, &[peer], crate::db::chat_store::SaveMode::Insert).unwrap();
    let items=(0..count).map(|n|json!({"id":n.to_string(),"uri":format!("fsid:remote-{n}"),"fileName":"fixture.txt","size":3})).collect::<Vec<_>>();
    let mut chat = crate::db::DChat::new(
        "me",
        "peer",
        "",
        &json!({"type":"FILES","value":{"items":items}}).to_string(),
    );
    chat.id = "message".into();
    db.insert_chat(&chat);
    let queue = Queue::new(
        db.clone(),
        dir.path().to_path_buf(),
        Arc::new(Imports::default()),
    );
    (dir, db, queue)
}
fn status(queue: &Queue, id: &str) -> String {
    queue.snapshot()["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == id)
        .unwrap()["status"]
        .as_str()
        .unwrap()
        .into()
}
fn start(queue: &Queue) -> (Snapshot, Ticket) {
    match queue.effect().unwrap() {
        Effect::Start { task, ticket } => (task, ticket),
        _ => panic!("expected start"),
    }
}
#[test]
fn fifo_three_slots_pause_cancel_and_stale_receipts() {
    let (_dir, _db, q) = fixture(5);
    for n in 0..5 {
        assert!(q.enqueue("message", &n.to_string(), "peer").unwrap());
    }
    let (old, _) = start(&q);
    assert_eq!(old.id, "0");
    assert_eq!(start(&q).0.id, "1");
    assert_eq!(start(&q).0.id, "2");
    assert!(q.effect().is_none());
    assert_eq!(status(&q, "3"), "PENDING");
    assert!(!q.enqueue("message", "0", "peer").unwrap());
    assert!(q.control("0", "pause").unwrap());
    assert!(matches!(q.effect().unwrap(),Effect::Cancel{token} if token==old.generation));
    let (fourth, ticket) = start(&q);
    assert_eq!(fourth.id, "3");
    assert!(!q.progress(&old.id, &old.generation, 3).unwrap());
    assert!(!q.finish(&old.id, &old.generation, None).unwrap());
    assert_eq!(status(&q, "0"), "PAUSED");
    std::fs::write(&ticket.path, b"abc").unwrap();
    assert!(q.finish(&fourth.id, &fourth.generation, None).unwrap());
    assert_eq!(start(&q).0.id, "4");
    assert!(q.control("0", "resume").unwrap());
    assert_eq!(status(&q, "0"), "PENDING");
    assert!(q.control("1", "cancel").unwrap());
    assert!(matches!(q.effect().unwrap(), Effect::Cancel { .. }));
    let (new, _) = start(&q);
    assert_eq!(new.id, "0");
    assert_ne!(old.generation, new.generation);
    assert!(
        !q.finish(&old.id, &old.generation, Some("late".into()))
            .unwrap()
    );
    assert_eq!(status(&q, "0"), "DOWNLOADING");
}
#[test]
fn bytes_are_monotonic_and_sql_failure_never_completes() {
    let (_dir, db, q) = fixture(1);
    q.enqueue("message", "0", "peer").unwrap();
    let (task, ticket) = start(&q);
    assert!(q.progress("0", &task.generation, 2).unwrap());
    assert!(q.progress("0", &task.generation, 1).is_err());
    assert!(q.progress("0", &task.generation, 4).is_err());
    std::fs::write(&ticket.path, b"abc").unwrap();
    db.with_conn(|c|c.execute_batch("CREATE TRIGGER reject BEFORE INSERT ON app_files BEGIN SELECT RAISE(ABORT,'fixture'); END;")).unwrap();
    q.finish("0", &task.generation, None).unwrap();
    assert_eq!(status(&q, "0"), "FAILED");
    assert!(!std::path::Path::new(&ticket.path).exists());
    assert_eq!(
        db.with_conn(|c| c.query_row("SELECT count(*) FROM app_files", [], |r| r.get::<_, i64>(0)))
            .unwrap(),
        0
    );
    assert!(q.control("0", "retry").unwrap());
    let (new, ticket) = start(&q);
    assert_ne!(new.generation, task.generation);
    assert!(!q.progress("0", &task.generation, 3).unwrap());
    assert_eq!(q.snapshot()["tasks"][0]["downloaded"], 0);
    db.with_conn(|c| c.execute_batch("DROP TRIGGER reject"))
        .unwrap();
    std::fs::write(&ticket.path, b"abc").unwrap();
    q.finish("0", &new.generation, None).unwrap();
    assert_eq!(status(&q, "0"), "COMPLETED");
    assert!(!q.control("0", "pause").unwrap());
    assert!(!q.finish("0", &new.generation, None).unwrap());
}
#[test]
fn changed_or_deleted_message_and_short_file_fail_without_import() {
    for mode in ["changed", "deleted", "short"] {
        let (_dir, db, q) = fixture(1);
        q.enqueue("message", "0", "peer").unwrap();
        let (task, ticket) = start(&q);
        std::fs::write(
            &ticket.path,
            if mode == "short" {
                b"ab".as_slice()
            } else {
                b"abc".as_slice()
            },
        )
        .unwrap();
        if mode == "deleted" {
            db.with_conn(|c| c.execute("DELETE FROM chats WHERE id='message'", []))
                .unwrap();
        }
        if mode == "changed" {
            db.with_conn(|c|c.execute("UPDATE chats SET content=json_set(content,'$.value.items[0].uri','fsid:changed') WHERE id='message'",[])).unwrap();
        }
        q.finish("0", &task.generation, None).unwrap();
        assert_eq!(status(&q, "0"), "FAILED");
        assert!(!std::path::Path::new(&ticket.path).exists());
        assert_eq!(
            db.with_conn(
                |c| c.query_row("SELECT count(*) FROM app_files", [], |r| r.get::<_, i64>(0))
            )
            .unwrap(),
            0
        );
    }
}
#[test]
fn remove_recreate_stale_generation_and_drop_clean_owned_temporary_files() {
    let (_dir, _db, q) = fixture(1);
    q.enqueue("message", "0", "peer").unwrap();
    let (old, old_ticket) = start(&q);
    assert!(q.control("0", "remove").unwrap());
    assert!(!std::path::Path::new(&old_ticket.path).exists());
    assert!(matches!(q.effect(), Some(Effect::Cancel { .. })));
    q.enqueue("message", "0", "peer").unwrap();
    let (new, ticket) = start(&q);
    assert_ne!(new.generation, old.generation);
    assert!(!q.finish("0", &old.generation, Some("late".into())).unwrap());
    assert!(std::path::Path::new(&ticket.path).exists());
    drop(q);
    assert!(!std::path::Path::new(&ticket.path).exists());
}
#[test]
fn queued_tasks_recheck_current_peer_and_attachment_before_start() {
    let (_dir, db, q) = fixture(4);
    for n in 0..4 {
        q.enqueue("message", &n.to_string(), "peer").unwrap();
    }
    let (first, _) = start(&q);
    start(&q);
    start(&q);
    db.with_conn(|c| c.execute("DELETE FROM peers WHERE id='peer'", []))
        .unwrap();
    q.finish(&first.id, &first.generation, Some("offline".into()))
        .unwrap();
    assert_eq!(status(&q, "3"), "FAILED");
    assert!(q.effect().is_none());
    let public = q.public_progress();
    assert!(!public.contains("peer"));
    assert!(!public.contains("generation"));
    assert!(public.contains("failed"));
}
#[test]
fn registry_and_effect_capacity_are_bounded() {
    let (_dir, _db, q) = fixture(129);
    for n in 0..128 {
        assert!(q.enqueue("message", &n.to_string(), "peer").unwrap());
    }
    assert!(q.enqueue("message", "128", "peer").is_err());
    let (_effects_dir, _effects_db, q) = fixture(1);
    q.enqueue("message", "0", "peer").unwrap();
    let mut count = 0;
    loop {
        if q.control("0", "remove").is_err() {
            break;
        }
        q.enqueue("message", "0", "peer").unwrap();
        count += 1;
        if count > 300 {
            panic!("unbounded effects");
        }
    }
    assert!(count <= 250);
}
