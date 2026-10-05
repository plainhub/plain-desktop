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
use tokio_tungstenite::{connect_async, tungstenite::Message};

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

#[tokio::test]
async fn rust_outgoing_reconnects_same_address_without_native_and_stops_without_revival() {
    use crate::db::chat_store::{SaveMode, peers};
    async fn wait_online(server: &ContentServer, id: &str, online: bool) {
        tokio::time::timeout(Duration::from_secs(12), async {
            loop {
                let row = server
                    .runtime_state()
                    .peer_status
                    .connections
                    .snapshot(&server.runtime_state().db);
                if row["online"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|value| value.as_str() == Some(id))
                    == online
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
    }
    fn configured(dir: &std::path::Path, id: &str) -> (Arc<crate::prefs::Prefs>, Vec<u8>) {
        let prefs = Arc::new(crate::prefs::Prefs::load(&dir.join("system.json")).unwrap());
        let (kp, pk) = crate::ed25519_generate();
        prefs.set_user("service", true).unwrap();
        prefs.set("client_id", id).unwrap();
        prefs.set("signature_key_pair",json!({"privateKey":crate::base64_encode(&kp[..32]),"publicKey":crate::base64_encode(&pk)}).to_string()).unwrap();
        (prefs, pk.to_vec())
    }
    let a_dir = tempfile::tempdir().unwrap();
    let b_dir = tempfile::tempdir().unwrap();
    let (a_prefs, a_pk) = configured(a_dir.path(), "a");
    let (b_prefs, b_pk) = configured(b_dir.path(), "b");
    let shared = crate::base64_encode(&[17; 32]);
    let a_path = a_dir.path().join("plain.db");
    let b_path = b_dir.path().join("plain.db");
    let a_db = Db::open(&a_path).unwrap();
    let b_db = Db::open(&b_path).unwrap();
    let mut a_peer = DPeer::new("a", "a", "127.0.0.1", 443, DeviceType::Phone);
    a_peer.key = shared.clone();
    a_peer.public_key = crate::base64_encode(&a_pk);
    a_peer.status = PeerStatus::Paired;
    peers::save(&b_db, &[a_peer], SaveMode::Insert).unwrap();
    let b =
        ContentServer::start(&b_path, &crate::base64_encode(&[12; 32]), b_prefs.clone()).unwrap();
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let (_, b_https) = b
        .start_public(
            0,
            0,
            cert.cert.pem().into_bytes(),
            cert.key_pair.serialize_pem().into_bytes(),
        )
        .await
        .unwrap();
    let mut b_peer = DPeer::new("b", "b", "127.0.0.1", b_https, DeviceType::Phone);
    b_peer.key = shared;
    b_peer.public_key = crate::base64_encode(&b_pk);
    b_peer.status = PeerStatus::Paired;
    peers::save(&a_db, &[b_peer.clone()], SaveMode::Insert).unwrap();
    let a_token = crate::base64_encode(&[13; 32]);
    let a = ContentServer::start(&a_path, &a_token, a_prefs).unwrap();
    a.start_public(
        0,
        0,
        cert.cert.pem().into_bytes(),
        cert.key_pair.serialize_pem().into_bytes(),
    )
    .await
    .unwrap();
    wait_online(&a, "b", true).await;
    wait_online(&b, "a", true).await;
    b.stop_public().await;
    wait_online(&a, "b", false).await;
    b.start_public(
        0,
        b_https,
        cert.cert.pem().into_bytes(),
        cert.key_pair.serialize_pem().into_bytes(),
    )
    .await
    .unwrap();
    wait_online(&a, "b", true).await;
    b_peer.status = PeerStatus::Unpaired;
    peers::save(&a_db, &[b_peer.clone()], SaveMode::Update).unwrap();
    wait_online(&a, "b", false).await;
    wait_online(&b, "a", false).await;
    b_peer.status = PeerStatus::Paired;
    peers::save(&a_db, &[b_peer], SaveMode::Update).unwrap();
    wait_online(&a, "b", true).await;
    let client = reqwest::Client::new();
    let local = format!("http://127.0.0.1:{}/chat/peer-status", a.port);
    let command = |action: &str| {
        client
            .post(&local)
            .bearer_auth(&a_token)
            .header("content-type", "application/json")
            .body(json!({"action":action}).to_string())
    };
    assert_eq!(command("stop").send().await.unwrap().status(), 200);
    wait_online(&a, "b", false).await;
    wait_online(&b, "a", false).await;
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert_eq!(
        a.runtime_state()
            .peer_status
            .connections
            .snapshot(&a.runtime_state().db)["online"],
        json!([])
    );
    assert_eq!(command("start").send().await.unwrap().status(), 200);
    wait_online(&a, "b", true).await;
    a.stop_public().await;
    assert_eq!(command("start").send().await.unwrap().status(), 400);
    wait_online(&a, "b", false).await;
    a.shutdown().await;
    b.shutdown().await;
}
