use super::*;
use serde_json::Value;
#[tokio::test]
async fn host_transport_preserves_signed_wire_and_requires_an_actual_receipt() {
    let host = Arc::new(Host::default());
    let (generation, mut receiver) = host.connect();
    let transport = Transport(
        host.clone(),
        Arc::new(crate::chat::transport_router::Router::default()),
    );
    let peer = DPeer::new(
        "fixture",
        "fixture",
        "127.0.0.1",
        443,
        crate::chat::enums::DeviceType::Phone,
    );
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
                    json!({"id":capabilities["id"],"result":["LAN"]}),
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
