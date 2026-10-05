use super::*;
use crate::{
    chat::enums::{DeviceType, PeerStatus},
    content_api::ContentServer,
    db::{
        DPeer, Db,
        chat_store::{SaveMode, peers},
    },
};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite_test::{connect_async, tungstenite::Message};

#[tokio::test]
async fn public_status_auth_scoped_connections_key_rotation_and_stop_use_rust_without_host() {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(crate::prefs::Prefs::load(&dir.path().join("system.json")).unwrap());
    prefs.set_user("service", true).unwrap();
    let path = dir.path().join("plain.db");
    let db = Db::open(&path).unwrap();
    let key = [8; 32];
    let (kp, pk) = crate::ed25519_generate();
    let mut peer = DPeer::new("peer", "phone", "127.0.0.1", 443, DeviceType::Phone);
    peer.status = PeerStatus::Paired;
    peer.key = crate::base64_encode(&key);
    peer.public_key = crate::base64_encode(&pk);
    peers::save(&db, &[peer.clone()], SaveMode::Insert).unwrap();
    let token = crate::base64_encode(&[1; 32]);
    let server = ContentServer::start(&path, &token, prefs.clone()).unwrap();
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let (http, _) = server
        .start_public(
            0,
            0,
            cert.cert.pem().into_bytes(),
            cert.key_pair.serialize_pem().into_bytes(),
        )
        .await
        .unwrap();
    let client = reqwest::Client::new();
    let local = format!("http://127.0.0.1:{}/chat/peer-status", server.port);
    assert_eq!(
        client
            .post(&local)
            .header("content-type", "application/json")
            .body(json!({"action":"snapshot"}).to_string())
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        client
            .post(&local)
            .bearer_auth(&token)
            .header("content-type", "application/json")
            .body(json!({"action":"snapshot","online":true}).to_string())
            .send()
            .await
            .unwrap()
            .status(),
        422
    );
    async fn snapshot(client: &reqwest::Client, url: &str, token: &str) -> serde_json::Value {
        serde_json::from_str::<serde_json::Value>(
            &client
                .post(url)
                .bearer_auth(token)
                .header("content-type", "application/json")
                .body(json!({"action":"snapshot"}).to_string())
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap(),
        )
        .unwrap()["result"]
            .clone()
    }
    fn wire(key: &[u8], kp: &[u8], id: &str) -> Vec<u8> {
        crate::chat::peer_status::handshake(key, kp, id).unwrap()
    }
    let url = format!("ws://127.0.0.1:{http}/status?cid=peer");
    let (mut bad, _) = connect_async(&url).await.unwrap();
    bad.send(Message::Binary(wire(&key, &kp, "other").into()))
        .await
        .unwrap();
    assert!(!matches!(bad.next().await,Some(Ok(Message::Text(t))) if t=="ok"));
    assert_eq!(snapshot(&client, &local, &token).await["online"], json!([]));
    let (mut first, _) = connect_async(&url).await.unwrap();
    first
        .send(Message::Binary(wire(&key, &kp, "peer").into()))
        .await
        .unwrap();
    assert_eq!(
        first.next().await.unwrap().unwrap().into_text().unwrap(),
        "ok"
    );
    let (mut second, _) = connect_async(&url).await.unwrap();
    second
        .send(Message::Binary(wire(&key, &kp, "peer").into()))
        .await
        .unwrap();
    assert_eq!(
        second.next().await.unwrap().unwrap().into_text().unwrap(),
        "ok"
    );
    first.close(None).await.unwrap();
    assert_eq!(
        snapshot(&client, &local, &token).await["online"],
        json!(["peer"])
    );
    let new_key = [9; 32];
    let (new_kp, new_pk) = crate::ed25519_generate();
    peer.key = crate::base64_encode(&new_key);
    peer.public_key = crate::base64_encode(&new_pk);
    peers::save(&db, &[peer], SaveMode::Update).unwrap();
    assert_eq!(snapshot(&client, &local, &token).await["online"], json!([]));
    let (mut replacement, _) = connect_async(&url).await.unwrap();
    replacement
        .send(Message::Binary(wire(&new_key, &new_kp, "peer").into()))
        .await
        .unwrap();
    assert_eq!(
        replacement
            .next()
            .await
            .unwrap()
            .unwrap()
            .into_text()
            .unwrap(),
        "ok"
    );
    let _ = tokio::time::timeout(Duration::from_secs(3), second.next())
        .await
        .unwrap();
    assert_eq!(
        snapshot(&client, &local, &token).await["online"],
        json!(["peer"])
    );
    let mut stale = server.runtime_state().events.subscribe();
    server.stop_public().await;
    let _ = tokio::time::timeout(Duration::from_secs(3), replacement.next())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if snapshot(&client, &local, &token).await["online"] == json!([]) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(stale.recv().await.unwrap().event_type, 10002);
    server.shutdown().await;
}
