use super::*;
use futures::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message as WsMessage, client::IntoClientRequest},
};
fn fixture() -> (tempfile::TempDir, crate::assembly::server::ContentServer) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(plain_rs::prefs::Prefs::load(&dir.path().join("system.json")).unwrap());
    prefs.set("client_id", "actor").unwrap();
    prefs.set_user("service", true).unwrap();
    let server = crate::assembly::server::ContentServer::start(
        &dir.path().join("plain.db"),
        &plain_rs::utils::base64::base64_encode(&[3; 32]),
        prefs,
    )
    .unwrap();
    (dir, server)
}
fn socket_request(
    port: u16,
    path: &str,
    token: &str,
) -> tokio_tungstenite::tungstenite::http::Request<()> {
    let mut req = format!("ws://127.0.0.1:{port}{path}")
        .into_client_request()
        .unwrap();
    req.headers_mut()
        .insert("authorization", format!("Bearer {token}").parse().unwrap());
    req
}
#[tokio::test]
async fn outgoing_completes_in_one_host_call() {
    let (_dir, server) = fixture();
    let state = server.runtime_state();
    let (generation, mut requests) = state.host.connect();
    let task_state = state.clone();
    let peer = plain_rs::db::DPeer::new(
        "peer",
        "peer",
        "",
        443,
        plain_rs::chat::enums::DeviceType::Phone,
    );
    let task = tokio::spawn(async move {
        task_state
            .ble_transport
            .exchange(
                &task_state,
                &peer,
                Request::PeerGraphql {
                    client_id: "actor".into(),
                    channel_id: "频道".into(),
                    body: vec![0, 255],
                }
                .encode()
                .unwrap(),
            )
            .await
            .unwrap()
    });
    // Rust asks the host for the whole exchange in a single call, carrying the
    // encoded request — there is no websocket token/handshake in between.
    let call = requests.recv().await.unwrap();
    assert_eq!(call["method"], "peerTransportBleExchange");
    assert!(call["params"].get("token").is_none());
    let request =
        plain_rs::utils::base64::base64_decode(call["params"]["request"].as_str().unwrap());
    assert_eq!(
        Request::decode(&request).unwrap(),
        Request::PeerGraphql {
            client_id: "actor".into(),
            channel_id: "频道".into(),
            body: vec![0, 255]
        }
    );
    let reply = ble::response(403, &[255, 0]).unwrap();
    state
        .host
        .reply(
            generation,
            serde_json::json!({"id":call["id"],"result":{"response":plain_rs::utils::base64::base64_encode(&reply)}}),
        )
        .unwrap();
    assert_eq!(task.await.unwrap(), reply);
    state.host.disconnect(generation);
    server.shutdown().await;
}
#[tokio::test]
async fn incoming_preserves_file_bytes_without_host_bridge() {
    let (dir, server) = fixture();
    let state = server.runtime_state();
    let payload = (0..8192).map(|n| n as u8).collect::<Vec<_>>();
    let file = plain_rs::chat::attachments::app_file_store::import_bytes(
        &state.db,
        dir.path(),
        &payload,
        "image/jpeg",
    )
    .unwrap();
    let id = plain_rs::crypto::xchacha_encrypt(
        &plain_rs::prefs::ensure_url_token(&state.prefs),
        format!("fid:{}", file.fid_suffix).as_bytes(),
    )
    .unwrap();
    let (mut socket, _) = connect_async(socket_request(
        server.port,
        "/chat/ble-incoming",
        &plain_rs::utils::base64::base64_encode(&[3; 32]),
    ))
    .await
    .unwrap();
    socket.send(WsMessage::Text(serde_json::json!({"remoteHost":"actual MAC","characteristicUuid":"d8d5c4a0-8f0a-4e7d-b9e1-706c70616913"}).to_string().into())).await.unwrap();
    socket
        .send(WsMessage::Binary(
            Request::FileChunk {
                client_id: "actor".into(),
                file_id: plain_rs::utils::base64::base64_encode(&id),
                offset: 0,
                length: 8192,
            }
            .encode()
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();
    let WsMessage::Binary(bytes) = socket.next().await.unwrap().unwrap() else {
        panic!("Expected binary")
    };
    let (status, body) = ble::decode_response(&bytes).unwrap();
    assert_eq!(status, 200);
    assert_eq!(body, payload);
    assert!(!state.host.connected());
    server.shutdown().await;
}
#[tokio::test]
async fn local_channels_reject_bad_auth_and_wrong_frame_type() {
    let (_dir, server) = fixture();
    assert!(
        connect_async(socket_request(server.port, "/chat/ble-incoming", "wrong"))
            .await
            .is_err()
    );
    let (mut socket, _) = connect_async(socket_request(
        server.port,
        "/chat/ble-incoming",
        &plain_rs::utils::base64::base64_encode(&[3; 32]),
    ))
    .await
    .unwrap();
    socket
        .send(WsMessage::Binary(vec![0; 8].into()))
        .await
        .unwrap();
    let frame = tokio::time::timeout(Duration::from_secs(2), socket.next())
        .await
        .unwrap();
    assert!(matches!(
        frame,
        None | Some(Err(_)) | Some(Ok(WsMessage::Close(_)))
    ));
    server.shutdown().await;
}
