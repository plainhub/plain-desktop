#![cfg(feature = "http_transport")]
use super::*;
use serde_json::Value;
use std::sync::Arc;
#[tokio::test]
async fn host_transport_preserves_signed_wire_and_requires_an_actual_receipt() {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(crate::prefs::Prefs::load(&dir.path().join("system.json")).unwrap());
    let server = super::super::ContentServer::start(
        &dir.path().join("plain.db"),
        &crate::base64_encode(&[1; 32]),
        prefs,
    )
    .unwrap();
    let state = server.runtime_state();
    let host = state.host.clone();
    let (generation, mut receiver) = host.connect();
    let transport = Transport(state.clone());
    let peer = DPeer::new(
        "fixture",
        "fixture",
        "",
        443,
        crate::chat::enums::DeviceType::Phone,
    );
    crate::db::chat_store::peers::save(
        &state.db,
        &[peer.clone()],
        crate::db::chat_store::SaveMode::Insert,
    )
    .unwrap();
    let responder = {
        let host = host.clone();
        tokio::spawn(async move {
            for data in [
                json!({"createChatItem":[]}),
                json!({"createChatItem":null}),
                json!({"createChatItem":[]}),
            ] {
                let capabilities = receiver.recv().await.unwrap();
                assert_eq!(capabilities["method"], "peerTransportCapabilities");
                host.reply(
                    generation,
                    json!({"id":capabilities["id"],"result":["AWARE"]}),
                )
                .unwrap();
                let request = receiver.recv().await.unwrap();
                assert_eq!(request["method"], "peerTransportAttempt");
                assert_eq!(request["params"]["peer"]["id"], "fixture");
                assert_eq!(request["params"]["channelId"], "channel");
                assert_eq!(request["params"]["body"], "signed|123|request");
                assert_eq!(
                    crate::base64_decode(request["params"]["key"].as_str().unwrap()),
                    [7; 32]
                );
                let errors = if data == json!({"createChatItem":null}) {
                    Value::Null
                } else if data == json!({"createChatItem":[]}) && request["id"].as_u64() == Some(6)
                {
                    json!([{"message":"rejected"}])
                } else {
                    Value::Null
                };
                host.reply(
                    generation,
                    json!({"id":request["id"],"result":{"kind":"connected","response":{"data":data,"errors":errors}}}),
                )
                .unwrap();
            }
        })
    };
    assert!(
        transport
            .message(&peer, "self", "channel", &[7; 32], "signed|123|request")
            .await
            .is_ok()
    );
    assert!(
        transport
            .message(&peer, "self", "channel", &[7; 32], "signed|123|request")
            .await
            .is_err()
    );
    assert!(
        transport
            .message(&peer, "self", "channel", &[7; 32], "signed|123|request")
            .await
            .is_err()
    );
    responder.await.unwrap();
    host.disconnect(generation);
}
