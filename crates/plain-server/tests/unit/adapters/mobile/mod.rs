#![cfg(feature = "http_transport")]
use crate::ContentServer;
use plain_rs::chat::enums::DeviceType;
use plain_rs::chat::enums::PeerStatus;
use plain_rs::db::DPeer;
use plain_rs::db::chat_store::SaveMode;
use plain_rs::db::chat_store::peers;
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn root(id: &str) -> (tempfile::TempDir, ContentServer, Vec<u8>, String) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(plain_rs::prefs::Prefs::load(&dir.path().join("system.json")).unwrap());
    prefs.set("client_id", id).unwrap();
    prefs.set_user("service", true).unwrap();
    let (kp, pk) = plain_rs::crypto::ed25519_generate();
    prefs
        .set(
            "signature_key_pair",
            plain_rs::utils::base64::base64_encode(&kp),
        )
        .unwrap();
    let token = plain_rs::utils::base64::base64_encode(&[3; 32]);
    let server = ContentServer::start(&dir.path().join("plain.db"), &token, prefs).unwrap();
    (dir, server, kp.to_vec(), token)
}
fn peer(id: &str, port: u16, pk: &[u8], ip: &str) -> DPeer {
    let mut row = DPeer::new(id, id, ip, port, DeviceType::Phone);
    row.status = PeerStatus::Paired;
    row.key = plain_rs::utils::base64::base64_encode(&[7; 32]);
    row.public_key = plain_rs::utils::base64::base64_encode(pk);
    row
}
async fn public(server: &ContentServer) -> u16 {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    server
        .start_public(
            0,
            0,
            cert.cert.pem().into_bytes(),
            cert.key_pair.serialize_pem().into_bytes(),
        )
        .await
        .unwrap()
        .1
}
#[tokio::test]
async fn aware_host_only_moves_raw_socket_bytes_rust_performs_tls_crypto_and_graphql() {
    let (_a, a, kp, _) = root("a");
    let (_b, b, bkp, _) = root("b");
    let port = public(&b).await;
    let remote = peer("b", port, &bkp[32..], "");
    let state = a.runtime_state();
    peers::save(&state.db, &[remote.clone()], SaveMode::Insert).unwrap();
    peers::save(
        &b.runtime_state().db,
        &[peer("a", 443, &kp[32..], "")],
        SaveMode::Insert,
    )
    .unwrap();
    let (generation, mut requests) = state.host.connect();
    let host = state.host.clone();
    let closed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = closed.clone();
    let responder = tokio::spawn(async move {
        type Reader = Arc<tokio::sync::Mutex<tokio::net::tcp::OwnedReadHalf>>;
        type Writer = Arc<tokio::sync::Mutex<tokio::net::tcp::OwnedWriteHalf>>;
        let mut sockets: std::collections::HashMap<String, (Reader, Writer)> = Default::default();
        while let Some(request) = requests.recv().await {
            let token = request["params"]["token"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            match request["method"].as_str().unwrap() {
                "peerTransportSocketOpen" => {
                    let (read, write) = tokio::net::TcpStream::connect(("127.0.0.1", port))
                        .await
                        .unwrap()
                        .into_split();
                    sockets.insert(
                        token,
                        (
                            Arc::new(tokio::sync::Mutex::new(read)),
                            Arc::new(tokio::sync::Mutex::new(write)),
                        ),
                    );
                    host.reply(generation, json!({"id":request["id"],"result":true}))
                        .unwrap();
                }
                "peerTransportSocketRead" => {
                    let reader = sockets[&token].0.clone();
                    let host = host.clone();
                    tokio::spawn(async move {
                        let mut bytes =
                            vec![0; request["params"]["length"].as_u64().unwrap() as usize];
                        let n = tokio::time::timeout(
                            std::time::Duration::from_millis(200),
                            reader.lock().await.read(&mut bytes),
                        )
                        .await;
                        let value = match n {
                            Ok(Ok(n)) => {
                                json!({"eof":n==0,"bytes":plain_rs::utils::base64::base64_encode(&bytes[..n])})
                            }
                            _ => json!({"eof":false,"bytes":null}),
                        };
                        host.reply(generation, json!({"id":request["id"],"result":value}))
                            .unwrap();
                    });
                }
                "peerTransportSocketWrite" => {
                    let bytes = plain_rs::utils::base64::base64_decode(
                        request["params"]["bytes"].as_str().unwrap(),
                    );
                    sockets[&token]
                        .1
                        .lock()
                        .await
                        .write_all(&bytes)
                        .await
                        .unwrap();
                    host.reply(generation, json!({"id":request["id"],"result":bytes.len()}))
                        .unwrap();
                }
                "peerTransportSocketClose" => {
                    sockets.remove(&token);
                    observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    host.reply(generation, json!({"id":request["id"],"result":true}))
                        .unwrap();
                }
                "peerTransportSocketCloseAll" => {
                    sockets.clear();
                    host.reply(generation, json!({"id":request["id"],"result":true}))
                        .unwrap();
                }
                _ => panic!("{request}"),
            }
        }
    });
    let wire =
        plain_rs::transport::contract::signed_request(&kp, "query { __typename }", json!({}))
            .unwrap();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        super::peers::peer_sdk::aware_send(&state, &remote, "", &[7; 32], &wire),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(result["data"]["__typename"], "Query");
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while closed.load(std::sync::atomic::Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    a.shutdown().await;
    state.host.disconnect(generation);
    responder.abort();
    b.shutdown().await;
}
#[tokio::test]
async fn ble_incoming_file_uses_root_encrypted_fid_and_eof_without_native_http_bridge() {
    let (dir, server, _, _token) = root("local");
    let state = server.runtime_state();
    let imported = plain_rs::chat::attachments::app_file_store::import_bytes(
        &state.db,
        dir.path(),
        b"actual bytes",
        "text/plain",
    )
    .unwrap();
    let encrypted = plain_rs::crypto::xchacha_encrypt(
        &plain_rs::prefs::ensure_url_token(&state.prefs),
        format!("fid:{}", imported.fid_suffix).as_bytes(),
    )
    .unwrap();
    for (offset, length, expected) in [
        (0, 6, b"actual".as_slice()),
        (6, 8192, b" bytes".as_slice()),
        (12, 8192, b"".as_slice()),
    ] {
        let request = plain_rs::protocols::ble::Request::FileChunk {
            client_id: "peer".into(),
            file_id: plain_rs::utils::base64::base64_encode(&encrypted),
            offset,
            length,
        }
        .encode()
        .unwrap();
        let bytes = crate::services::discovery::ble_http::dispatch(
            state.clone(),
            "fixture BLE MAC".into(),
            &request,
        )
        .await
        .unwrap();
        let (status, body) = plain_rs::protocols::ble::decode_response(&bytes).unwrap();
        assert_eq!(status, 200);
        assert_eq!(body, expected);
    }
    assert!(!state.host.connected());
    server.shutdown().await;
}
#[tokio::test]
async fn aware_configuration_preserves_unpaired_channel_link_and_reads_current_root_key_and_port() {
    let (_dir, server, kp, token) = root("local");
    let state = server.runtime_state();
    let mut p = peer("remote", 443, &kp[32..], "");
    p.key.clear();
    peers::save(&state.db, &[p.clone()], SaveMode::Insert).unwrap();
    state.prefs.set_user("https_port", 2443).unwrap();
    async fn config(server: &ContentServer, token: &str) -> Value {
        let response = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{}/chat/transport", server.port))
            .bearer_auth(token)
            .header("content-type", "application/json")
            .body(json!({"action":"awareConfig","id":"remote"}).to_string())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        serde_json::from_slice::<Value>(&response.bytes().await.unwrap()).unwrap()["result"].clone()
    }
    let v = config(&server, &token).await;
    assert!(v["pmk"].is_null());
    assert_eq!(v["localPort"], 2443);
    assert_eq!(v["isClient"], true);
    p.key = plain_rs::utils::base64::base64_encode(&[9; 32]);
    peers::save(&state.db, &[p], SaveMode::Update).unwrap();
    state.prefs.set_user("https_port", 3443).unwrap();
    let v = config(&server, &token).await;
    assert_eq!(v["pmk"], plain_rs::utils::base64::base64_encode(&[9; 32]));
    assert_eq!(v["localPort"], 3443);
    server.shutdown().await;
}
