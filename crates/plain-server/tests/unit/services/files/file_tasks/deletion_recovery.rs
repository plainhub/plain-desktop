use super::*;
#[tokio::test]
async fn write_ahead_delete_recovers_missing_paths_without_repeating_physical_work() {
    use crate::assembly::server::ContentServer;
    use tokio_tungstenite::tungstenite::Message;
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("db");
    let token = plain_rs::utils::base64::base64_encode(&[21; 32]);
    let prefs = Arc::new(plain_rs::prefs::Prefs::load(&dir.path().join("prefs")).unwrap());
    let server = ContentServer::start(&db_path, &token, prefs).unwrap();
    let state = server.runtime_state();
    let path = dir.path().join("gone.txt").to_str().unwrap().to_owned();
    let kept = dir.path().join("kept.txt");
    std::fs::write(&kept, "keep").unwrap();
    let mut socket = crate::host::events::TestHost::new(&server);
    let host = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = socket.next().await {
            let request: serde_json::Value = serde_json::from_str(&text).unwrap();
            socket
                .send(Message::Text(
                    json!({"id":request["id"],"result":true}).to_string().into(),
                ))
                .await
                .unwrap();
        }
    });
    state.db.with_conn(|c|c.execute("INSERT INTO tag_relations(tag_id,key,type,created_at,size,title) VALUES ('tag',?1,22,'today',1,'title')",[&path])).unwrap();
    Intent {
        id: "receipt".into(),
        path: path.clone(),
        root: path.clone(),
        planned: vec![path.clone(), kept.to_str().unwrap().into()],
        snapshot: json!({"root":path,"items":[]}),
        outcome: None,
    }
    .save(&state.db)
    .unwrap();
    assert_eq!(state.files.recover_deletions().await.unwrap(), 1);
    assert_eq!(std::fs::read_to_string(kept).unwrap(), "keep");
    let count: i64 = state
        .db
        .with_conn(|c| c.query_row("SELECT count(*) FROM tag_relations", [], |r| r.get(0)))
        .unwrap();
    assert_eq!(count, 0);
    assert_eq!(state.files.recover_deletions().await.unwrap(), 0);
    host.abort();
    server.shutdown().await;
}
#[test]
fn deleted_path_replacement_preserves_the_receipt() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("replacement");
    std::fs::write(&path, "new").unwrap();
    assert!(require_absent(path.to_str().unwrap()).is_err());
}
