use super::*;
use serde_json::{Value, json};
#[cfg(feature = "http_transport")]
#[tokio::test]
async fn root_peer_views_addresses_and_file_urls_use_current_record_without_host() {
    let dir = tempfile::tempdir().unwrap();
    let prefs =
        std::sync::Arc::new(crate::prefs::Prefs::load(&dir.path().join("system.json")).unwrap());
    let token = crate::base64_encode(&[9; 32]);
    let server =
        super::super::ContentServer::start(&dir.path().join("plain.db"), &token, prefs).unwrap();
    let state = server.runtime_state();
    let mut peer = DPeer::new(
        "fixture",
        " ",
        "bad, 127.0.0.1",
        443,
        crate::chat::enums::DeviceType::Tablet,
    );
    peer.port = 443;
    crate::db::chat_store::peers::save(
        &state.db,
        &[peer.clone()],
        crate::db::chat_store::SaveMode::Insert,
    )
    .unwrap();
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/chat/store", server.port);
    let call = |body: Value| {
        client
            .post(&url)
            .bearer_auth(&token)
            .header("content-type", "application/json")
            .body(body.to_string())
    };
    async fn result(request: reqwest::RequestBuilder) -> Value {
        let response = request.send().await.unwrap();
        assert!(response.status().is_success());
        serde_json::from_str::<Value>(&response.text().await.unwrap()).unwrap()["result"].clone()
    }
    let row = result(call(json!({"action":"peer","id":"fixture"}))).await;
    assert_eq!(row["ip"], "bad, 127.0.0.1");
    assert_eq!(
        row["address"],
        json!({"bestIp":"127.0.0.1","name":"127.0.0.1","baseUrl":"https://127.0.0.1","apiUrl":"https://127.0.0.1/peer_graphql","statusWsUrl":"wss://127.0.0.1/status"})
    );
    let list = result(call(json!({"action":"peers","statuses":[]}))).await;
    assert_eq!(list[0], row);
    let selected = result(call(
        json!({"action":"peerAddress","id":"fixture","expected":peer}),
    ))
    .await;
    assert_eq!(selected, row["address"]);
    let id = "id +/?#&中文%";
    let file = result(call(
        json!({"action":"peerFileUrl","id":"fixture","expected":peer,"file_id":id}),
    ))
    .await;
    let parsed = reqwest::Url::parse(file.as_str().unwrap()).unwrap();
    assert_eq!(parsed.host_str(), Some("127.0.0.1"));
    assert_eq!(parsed.path(), "/fs");
    assert_eq!(
        parsed.query_pairs().collect::<Vec<_>>(),
        vec![("id".into(), id.into())]
    );
    let old = peer.clone();
    peer.ip = "::1".into();
    peer.port = 2443;
    crate::db::chat_store::peers::save(
        &state.db,
        &[peer.clone()],
        crate::db::chat_store::SaveMode::Update,
    )
    .unwrap();
    assert_eq!(
        call(json!({"action":"peerAddress","id":"fixture","expected":old}))
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    assert_eq!(
        call(json!({"action":"peerFileUrl","id":"fixture","expected":old,"file_id":id}))
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    let current = result(call(json!({"action":"peer","id":"fixture"}))).await;
    assert_eq!(
        current["address"]["apiUrl"],
        "https://[::1]:2443/peer_graphql"
    );
    assert_eq!(current["address"]["statusWsUrl"], "wss://[::1]:2443/status");
    crate::db::chat_store::peers::delete(&state.db, &["fixture".into()]).unwrap();
    assert_eq!(
        call(json!({"action":"peerAddress","id":"fixture","expected":peer}))
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    server.shutdown().await;
}
