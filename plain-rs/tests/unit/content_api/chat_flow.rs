#![cfg(feature = "http_transport")]
use super::ContentServer;
use crate::{
    chat::enums::{DeviceType, PeerStatus},
    db::{
        DPeer,
        chat_store::{SaveMode, peers},
    },
};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn root(id: &str) -> (tempfile::TempDir, ContentServer, Vec<u8>, String) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(crate::prefs::Prefs::load(&dir.path().join("system.json")).unwrap());
    prefs.set("client_id", id).unwrap();
    prefs.set_user("service", true).unwrap();
    let (kp, pk) = crate::ed25519_generate();
    prefs.set("signature_key_pair",json!({"privateKey":crate::base64_encode(&kp[..32]),"publicKey":crate::base64_encode(&pk)}).to_string()).unwrap();
    let token = crate::base64_encode(&[3; 32]);
    let server = ContentServer::start(&dir.path().join("plain.db"), &token, prefs).unwrap();
    (dir, server, kp.to_vec(), token)
}
fn peer(id: &str, port: u16, pk: &[u8], ip: &str) -> DPeer {
    let mut row = DPeer::new(id, id, ip, port, DeviceType::Phone);
    row.status = PeerStatus::Paired;
    row.key = crate::base64_encode(&[7; 32]);
    row.public_key = crate::base64_encode(pk);
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
async fn call(server: &ContentServer, token: &str, body: Value) -> Value {
    let response = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{}/chat/service", server.port))
        .bearer_auth(token)
        .header("content-type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .unwrap();
    let status = response.status();
    let value: Value = serde_json::from_slice(&response.bytes().await.unwrap()).unwrap();
    assert_eq!(status, 200, "{value}");
    value["result"].clone()
}
#[tokio::test]
async fn full_text_forward_query_and_delete_share_one_file_until_last_owner() {
    let (dir, server, _, token) = root("local");
    let state = server.runtime_state();
    let text = "😀".repeat(1024);
    let short = call(
        &server,
        &token,
        json!({"action":"sendText","targets":["local"],"text":text}),
    )
    .await;
    assert_eq!(
        serde_json::from_str::<Value>(short[0]["content"].as_str().unwrap()).unwrap()["type"],
        "TEXT"
    );
    let text = format!("{text}中");
    let rows = call(
        &server,
        &token,
        json!({"action":"sendText","targets":["local","local"],"text":text}),
    )
    .await;
    let content: Value = serde_json::from_str(rows[0]["content"].as_str().unwrap()).unwrap();
    let item = &content["value"]["items"][0];
    let suffix = item["uri"].as_str().unwrap().strip_prefix("fid:").unwrap();
    let hash = suffix.split('.').next().unwrap();
    assert_eq!(item["size"], text.len());
    assert_eq!(
        item["summary"].as_str().unwrap().encode_utf16().count(),
        250
    );
    assert_eq!(state.db.app_file_get(hash).unwrap().unwrap().ref_count, 2);
    let record = state.db.app_file_get(hash).unwrap().unwrap();
    assert_eq!(
        std::fs::read(dir.path().join(record.real_path)).unwrap(),
        text.as_bytes()
    );
    let forward = call(
        &server,
        &token,
        json!({"action":"forward","target":"local","id":rows[0]["id"]}),
    )
    .await;
    assert_eq!(state.db.app_file_get(hash).unwrap().unwrap().ref_count, 3);
    let selected = call(
        &server,
        &token,
        json!({"action":"items","target":"peer:local","offset":0,"limit":2,"query":"text:message"}),
    )
    .await;
    assert_eq!(selected.as_array().unwrap().len(), 2);
    for id in [rows[0]["id"].clone(), rows[1]["id"].clone()] {
        call(&server, &token, json!({"action":"delete","ids":[id]})).await;
        assert!(state.db.app_file_get(hash).unwrap().is_some());
    }
    call(
        &server,
        &token,
        json!({"action":"deleteQuery","query":format!("ids:{}",forward["id"].as_str().unwrap())}),
    )
    .await;
    assert!(state.db.app_file_get(hash).unwrap().is_none());
    server.shutdown().await;
}
#[tokio::test]
async fn picked_sources_are_imported_once_caption_is_separate_and_deleted_placeholders_release_imports()
 {
    let (dir, server, _, token) = root("local");
    let state = server.runtime_state();
    let (host_generation, mut requests) = state.host.connect();
    let host = state.host.clone();
    let db = state.db.clone();
    let responder = tokio::spawn(async move {
        let mut reads = 0;
        while let Some(request) = requests.recv().await {
            let result = match request["method"].as_str().unwrap() {
                "chatPickedFacts" => {
                    json!([{"uri":"content://fixture","name":"fixture.txt","size":99,"mimeType":"text/plain"}])
                }
                "chatPickedRead" => {
                    reads += 1;
                    std::fs::write(request["params"]["path"].as_str().unwrap(), b"abc").unwrap();
                    if reads == 2 {
                        crate::chat::app_file_store::chat_deletion::delete(
                            &db,
                            dir.path(),
                            crate::chat::app_file_store::chat_deletion::Selection::Peer("local"),
                        )
                        .unwrap();
                    }
                    json!({"width":0,"height":0,"durationMs":0})
                }
                "chatPickedRelease" => {
                    let path = request["params"]["path"].as_str().unwrap();
                    std::fs::remove_file(path).unwrap();
                    json!(true)
                }
                _ => panic!("{request}"),
            };
            host.reply(host_generation, json!({"id":request["id"],"result":result}))
                .unwrap();
        }
    });
    let result=call(&server,&token,json!({"action":"share","targets":["local","local"],"uris":["content://fixture","content://fixture"],"text":null,"caption":"caption","images":null,"normalize":false})).await;
    assert_eq!(result["ok"], true);
    assert_eq!(result["chats"].as_array().unwrap().len(), 4);
    let content: Value =
        serde_json::from_str(result["chats"][0]["content"].as_str().unwrap()).unwrap();
    let item = &content["value"]["items"][0];
    assert_eq!(item["size"], 3);
    let id = item["uri"].as_str().unwrap()[4..]
        .split('.')
        .next()
        .unwrap();
    assert_eq!(state.db.app_file_get(id).unwrap().unwrap().ref_count, 2);
    let result=call(&server,&token,json!({"action":"share","targets":["local"],"uris":["content://fixture"],"text":null,"caption":null,"images":false,"normalize":false})).await;
    assert_eq!(result["ok"], false);
    assert!(state.db.app_file_get(id).unwrap().is_none());
    state.host.disconnect(host_generation);
    responder.abort();
    server.shutdown().await;
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
                                json!({"eof":n==0,"bytes":crate::base64_encode(&bytes[..n])})
                            }
                            _ => json!({"eof":false,"bytes":null}),
                        };
                        host.reply(generation, json!({"id":request["id"],"result":value}))
                            .unwrap();
                    });
                }
                "peerTransportSocketWrite" => {
                    let bytes = crate::base64_decode(request["params"]["bytes"].as_str().unwrap());
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
        crate::chat::transport::signed_request(&kp, "query { __typename }", json!({})).unwrap();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        super::peer_sdk::aware_send(&state, &remote, "", &[7; 32], &wire),
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
    let (dir, server, _, token) = root("local");
    let state = server.runtime_state();
    let imported = crate::chat::app_file_store::import_bytes(
        &state.db,
        dir.path(),
        b"actual bytes",
        "text/plain",
    )
    .unwrap();
    let encrypted = crate::xchacha_encrypt(
        &crate::prefs::ensure_url_token(&state.prefs),
        format!("fid:{}", imported.fid_suffix).as_bytes(),
    )
    .unwrap();
    for (offset, length, expected) in [
        (0, 6, b"actual".as_slice()),
        (6, 8192, b" bytes".as_slice()),
        (12, 8192, b"".as_slice()),
    ] {
        let body = json!({"m":"GET","p":"/fs","q":{"id":[crate::base64_encode(&encrypted)],"offset":[offset.to_string()],"length":[length.to_string()]}});
        let response = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{}/chat/ble-http", server.port))
            .bearer_auth(&token)
            .header("content-type", "application/json")
            .body(json!({"body":body.to_string(),"headers":{"c-id":"peer"},"remote_host":"fixture BLE MAC"}).to_string())
            .send()
            .await
            .unwrap();
        let status = response.status();
        let value: Value = serde_json::from_slice(&response.bytes().await.unwrap()).unwrap();
        assert_eq!(status, 200, "{value}");
        let compact: Value = serde_json::from_str(value["result"].as_str().unwrap()).unwrap();
        assert_eq!(compact["s"], 200);
        assert_eq!(
            crate::base64_decode(compact["b"].as_str().unwrap()),
            expected
        );
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
    p.key = crate::base64_encode(&[9; 32]);
    peers::save(&state.db, &[p], SaveMode::Update).unwrap();
    state.prefs.set_user("https_port", 3443).unwrap();
    let v = config(&server, &token).await;
    assert_eq!(v["pmk"], crate::base64_encode(&[9; 32]));
    assert_eq!(v["localPort"], 3443);
    server.shutdown().await;
}
#[test]
fn share_classification_keeps_gallery_formats_and_opaque_names() {
    assert!(crate::chat::share_send::visual("gallery.APNG"));
    assert!(crate::chat::share_send::image("gallery.APNG"));
    assert!(crate::chat::share_send::visual("movie.3gpp"));
    assert!(!crate::chat::share_send::visual("icon.ico"));
    assert!(!crate::chat::share_send::visual("source.ts"));
    let facts = [crate::chat::share_send::PickedFile {
        uri: "content://fixture".into(),
        name: "gallery.APNG".into(),
        size: 5,
        mime_type: String::new(),
    }];
    assert_eq!(
        crate::chat::share_send::placeholders(&facts, true).unwrap()[0]["fileName"],
        "gallery.APNG"
    );
}
#[tokio::test]
async fn public_retry_returns_committed_pending_before_root_owned_delivery_receipt() {
    let (_dir, server, kp, token) = root("local");
    let state = server.runtime_state();
    let remote = peer("remote", 443, &kp[32..], "");
    peers::save(&state.db, &[remote], SaveMode::Insert).unwrap();
    let row = crate::chat::message_lifecycle::create(
        &state.db,
        "remote",
        "",
        r#"{"type":"TEXT","value":{"text":"retry"}}"#,
    )
    .unwrap();
    let (generation, mut requests) = state.host.connect();
    let result = call(&server, &token, json!({"action":"retry","id":row.id})).await;
    assert_eq!(result["status"], "PENDING");
    let caps = requests.recv().await.unwrap();
    state
        .host
        .reply(generation, json!({"id":caps["id"],"result":["BLE"]}))
        .unwrap();
    let exchange = requests.recv().await.unwrap();
    let bytes = crate::xchacha_encrypt_raw(&[7; 32], br#"{"data":{"createChatItem":[]}}"#).unwrap();
    state.host.reply(generation,json!({"id":exchange["id"],"result":json!({"s":200,"b":crate::base64_encode(&bytes)}).to_string()})).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if crate::db::chat_store::messages::get(&state.db, &row.id)
                .unwrap()
                .unwrap()
                .status
                == crate::chat::enums::ChatStatus::Sent
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    state.host.disconnect(generation);
    server.shutdown().await;
}
