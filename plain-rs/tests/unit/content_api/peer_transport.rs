#![cfg(feature = "http_transport")]
use super::*;
use std::sync::Arc;
fn peer() -> DPeer {
    DPeer::new(
        "fixture",
        "fixture",
        "",
        443,
        crate::chat::enums::DeviceType::Phone,
    )
}
fn state() -> (tempfile::TempDir, super::super::ContentServer, ServerState) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(crate::prefs::Prefs::load(&dir.path().join("system.json")).unwrap());
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
async fn fallback_is_rust_owned_and_graphql_errors_are_terminal_connected_responses() {
    let (_dir, _server, state) = state();
    let host = state.host.clone();
    let (generation, mut receiver) = host.connect();
    let responder = {
        let host = host.clone();
        tokio::spawn(async move {
            let capabilities = receiver.recv().await.unwrap();
            assert_eq!(capabilities["method"], "peerTransportCapabilities");
            host.reply(
                generation,
                json!({"id":capabilities["id"],"result":["BLE","AWARE","LAN"]}),
            )
            .unwrap();
            let aware = receiver.recv().await.unwrap();
            assert_eq!(aware["params"]["transport"], "AWARE");
            assert_eq!(aware["params"]["body"], "signed request");
            host.reply(generation,json!({"id":aware["id"],"result":{"kind":"connected","response":{"errors":[{"message":"business rejection"}]}}})).unwrap();
        })
    };
    let value = send(&state, &peer(), "channel", &[7; 32], "signed request")
        .await
        .unwrap();
    assert!(value["errors"].is_array());
    responder.await.unwrap();
    host.disconnect(generation);
}
#[tokio::test]
async fn cancellation_releases_route_and_fatal_host_errors_do_not_fall_back() {
    let (_dir, _server, state) = state();
    let host = state.host.clone();
    let (generation, mut receiver) = host.connect();
    let call = {
        let state = state.clone();
        tokio::spawn(async move { send(&state, &peer(), "", &[7; 32], "signed").await })
    };
    let capabilities = receiver.recv().await.unwrap();
    host.reply(
        generation,
        json!({"id":capabilities["id"],"result":["AWARE"]}),
    )
    .unwrap();
    let _attempt = receiver.recv().await.unwrap();
    call.abort();
    assert!(call.await.unwrap_err().is_cancelled());
    // All 64 slots remain available after cancellation.
    let mut tickets = Vec::new();
    for _ in 0..64 {
        tickets.push(
            state
                .transport
                .begin(&peer(), &[TransportType::Aware])
                .unwrap()
                .ticket
                .unwrap(),
        );
    }
    for ticket in tickets {
        state.transport.abort(&ticket);
    }
    let call = {
        let state = state.clone();
        tokio::spawn(async move { send(&state, &peer(), "", &[7; 32], "signed").await })
    };
    let capabilities = receiver.recv().await.unwrap();
    host.reply(
        generation,
        json!({"id":capabilities["id"],"result":["AWARE","BLE"]}),
    )
    .unwrap();
    let attempt = receiver.recv().await.unwrap();
    host.reply(
        generation,
        json!({"id":attempt["id"],"error":"response authentication failed"}),
    )
    .unwrap();
    assert_eq!(
        call.await.unwrap().unwrap_err(),
        "response authentication failed"
    );
    host.disconnect(generation);
}
