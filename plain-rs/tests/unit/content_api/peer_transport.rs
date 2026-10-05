#![cfg(feature = "http_transport")]
use super::*;
use std::sync::Arc;
fn peer() -> DPeer {
    let mut p = DPeer::new(
        "fixture",
        "fixture",
        "",
        443,
        crate::chat::enums::DeviceType::Phone,
    );
    p.key = crate::base64_encode(&[7; 32]);
    p
}
fn state() -> (tempfile::TempDir, super::super::ContentServer, ServerState) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(crate::prefs::Prefs::load(&dir.path().join("system.json")).unwrap());
    prefs.set("client_id", "actor").unwrap();
    let server = super::super::ContentServer::start(
        &dir.path().join("plain.db"),
        &crate::base64_encode(&[1; 32]),
        prefs,
    )
    .unwrap();
    let state = server.runtime_state();
    peers::save(
        &state.db,
        &[peer()],
        crate::db::chat_store::SaveMode::Insert,
    )
    .unwrap();
    (dir, server, state)
}
#[tokio::test]
async fn rust_falls_back_from_unavailable_socket_and_authenticates_raw_ble_business_errors() {
    let (_dir, server, state) = state();
    let host = state.host.clone();
    let (generation, mut receiver) = host.connect();
    let responder = tokio::spawn(async move {
        while let Some(request) = receiver.recv().await {
            let response = match request["method"].as_str().unwrap() {
                "peerTransportCapabilities" => json!({"id":request["id"],"result":["BLE","AWARE"]}),
                "peerTransportSocketOpen" => {
                    json!({"id":request["id"],"error":"SDK network unavailable"})
                }
                "peerTransportSocketClose" | "peerTransportSocketCloseAll" => {
                    json!({"id":request["id"],"result":true})
                }
                "peerTransportBleExchange" => {
                    assert_eq!(request["params"]["headers"]["c-id"], "actor");
                    assert_eq!(request["params"]["headers"]["c-cid"], "channel");
                    let body: Value =
                        serde_json::from_str(request["params"]["body"].as_str().unwrap()).unwrap();
                    assert_eq!(body["p"], "/peer_graphql");
                    assert_eq!(body["bb"], true);
                    assert_eq!(
                        crate::xchacha_decrypt_raw(
                            &[7; 32],
                            &crate::base64_decode(body["b"].as_str().unwrap())
                        )
                        .unwrap(),
                        b"signed request"
                    );
                    let bytes = crate::xchacha_encrypt_raw(
                        &[7; 32],
                        br#"{"errors":[{"message":"business rejection"}]}"#,
                    )
                    .unwrap();
                    json!({"id":request["id"],"result":json!({"s":200,"b":crate::base64_encode(&bytes)}).to_string()})
                }
                _ => panic!("{request}"),
            };
            host.reply(generation, response).unwrap();
        }
    });
    let response = send(&state, &peer(), "channel", &[7; 32], "signed request")
        .await
        .unwrap();
    assert_eq!(response["errors"][0]["message"], "business rejection");
    assert!(state.transport.active().is_empty());
    server.shutdown().await;
    state.host.disconnect(generation);
    responder.abort();
}
#[tokio::test]
async fn cancel_releases_route_and_malformed_raw_ble_response_is_terminal() {
    let (_dir, server, state) = state();
    let host = state.host.clone();
    let (generation, mut receiver) = host.connect();
    let request_state = state.clone();
    let call =
        tokio::spawn(async move { send(&request_state, &peer(), "", &[7; 32], "signed").await });
    let caps = receiver.recv().await.unwrap();
    host.reply(generation, json!({"id":caps["id"],"result":["BLE"]}))
        .unwrap();
    let _exchange = receiver.recv().await.unwrap();
    call.abort();
    assert!(call.await.unwrap_err().is_cancelled());
    assert!(state.transport.active().is_empty());
    let request_state = state.clone();
    let call =
        tokio::spawn(async move { send(&request_state, &peer(), "", &[7; 32], "signed").await });
    let caps = receiver.recv().await.unwrap();
    host.reply(generation, json!({"id":caps["id"],"result":["BLE"]}))
        .unwrap();
    let exchange = receiver.recv().await.unwrap();
    host.reply(
        generation,
        json!({"id":exchange["id"],"result":"malformed response"}),
    )
    .unwrap();
    assert!(call.await.unwrap().is_err());
    assert!(state.transport.active().is_empty());
    host.disconnect(generation);
    server.shutdown().await;
}
