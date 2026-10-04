use super::*;
fn fixture() -> (tempfile::TempDir, Arc<Queue>) {
    let dir = tempfile::tempdir().unwrap();
    let db = crate::db::Db::open(&dir.path().join("plain.db")).unwrap();
    let peer = crate::db::DPeer::new(
        "peer",
        "fixture",
        "127.0.0.1",
        443,
        crate::chat::enums::DeviceType::Phone,
    );
    crate::db::chat_store::peers::save(&db, &[peer], crate::db::chat_store::SaveMode::Insert)
        .unwrap();
    let items=(0..4).map(|n|json!({"id":n.to_string(),"uri":format!("fsid:{n}"),"fileName":"fixture.txt","size":3})).collect::<Vec<_>>();
    let mut chat = crate::db::DChat::new(
        "me",
        "peer",
        "",
        &json!({"type":"FILES","value":{"items":items}}).to_string(),
    );
    chat.id = "message".into();
    db.insert_chat(&chat);
    let q = Arc::new(Queue::new(
        db,
        dir.path().to_path_buf(),
        Arc::new(crate::chat::attachment_imports::Imports::default()),
    ));
    (dir, q)
}
#[tokio::test]
async fn host_start_cancel_effects_are_ordered_and_events_remain_public() {
    let (_dir, q) = fixture();
    let host = Arc::new(Host::default());
    let (generation, mut receiver) = host.connect();
    let (events, mut event) = broadcast::channel(32);
    let (stop, stopped) = watch::channel(false);
    start(&q, host.clone(), events, stopped);
    for n in 0..4 {
        q.enqueue("message", &n.to_string(), "peer").unwrap();
    }
    let old = tokio::time::timeout(std::time::Duration::from_secs(5), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(old["method"], "attachmentTransferStart");
    assert_eq!(old["params"]["task"]["id"], "0");
    q.control("0", "pause").unwrap();
    host.reply(generation, json!({"id":old["id"],"result":true}))
        .unwrap();
    for n in ["1", "2"] {
        let next = receiver.recv().await.unwrap();
        assert_eq!(next["params"]["task"]["id"], n);
        host.reply(generation, json!({"id":next["id"],"result":true}))
            .unwrap();
    }
    let cancel = receiver.recv().await.unwrap();
    assert_eq!(cancel["method"], "attachmentTransferCancel");
    assert_eq!(
        cancel["params"]["token"],
        old["params"]["task"]["generation"]
    );
    host.reply(generation, json!({"id":cancel["id"],"result":true}))
        .unwrap();
    let next = receiver.recv().await.unwrap();
    assert_eq!(next["params"]["task"]["id"], "3");
    host.reply(generation, json!({"id":next["id"],"result":true}))
        .unwrap();
    let payload = tokio::time::timeout(std::time::Duration::from_secs(5), event.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        payload.event_type,
        crate::chat::events::WS_DOWNLOAD_PROGRESS
    );
    assert!(!payload.payload.contains("generation"));
    assert!(!payload.payload.contains("peer"));
    std::fs::write(next["params"]["path"].as_str().unwrap(), b"abc").unwrap();
    assert!(
        q.finish(
            "3",
            next["params"]["task"]["generation"].as_str().unwrap(),
            None
        )
        .unwrap()
    );
    assert!(
        !q.finish(
            "0",
            old["params"]["task"]["generation"].as_str().unwrap(),
            None
        )
        .unwrap()
    );
    stop.send(true).unwrap();
}
#[tokio::test]
async fn failed_or_false_host_ack_releases_slots_and_fails_task() {
    let (_dir, q) = fixture();
    let host = Arc::new(Host::default());
    let (generation, mut receiver) = host.connect();
    let (events, _) = broadcast::channel(32);
    let (stop, stopped) = watch::channel(false);
    start(&q, host.clone(), events, stopped);
    for n in 0..4 {
        q.enqueue("message", &n.to_string(), "peer").unwrap();
    }
    for n in 0..4 {
        let request = tokio::time::timeout(std::time::Duration::from_secs(5), receiver.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(request["params"]["task"]["id"], n.to_string());
        let reply = if n == 0 {
            json!({"id":request["id"],"error":"fixture unavailable"})
        } else {
            json!({"id":request["id"],"result":false})
        };
        host.reply(generation, reply).unwrap();
    }
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while q.snapshot()["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["status"] != "FAILED")
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    stop.send(true).unwrap();
}
