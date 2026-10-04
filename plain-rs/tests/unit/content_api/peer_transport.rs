use super::*;
use std::sync::Arc;
fn peer() -> DPeer {
    DPeer::new(
        "fixture",
        "fixture",
        "127.0.0.1",
        443,
        crate::chat::enums::DeviceType::Phone,
    )
}
#[tokio::test]
async fn fallback_is_rust_owned_and_graphql_errors_are_terminal_connected_responses() {
    let host = Arc::new(Host::default());
    let router = Router::default();
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
            let lan = receiver.recv().await.unwrap();
            assert_eq!(lan["params"]["transport"], "LAN");
            assert_eq!(lan["params"]["timeoutMs"], 15_000);
            host.reply(
                generation,
                json!({"id":lan["id"],"result":{"kind":"unavailable","error":"offline"}}),
            )
            .unwrap();
            let aware = receiver.recv().await.unwrap();
            assert_eq!(aware["params"]["transport"], "AWARE");
            assert_eq!(aware["params"]["body"], "signed request");
            host.reply(generation,json!({"id":aware["id"],"result":{"kind":"connected","response":{"errors":[{"message":"business rejection"}]}}})).unwrap();
        })
    };
    let value = send(
        &host,
        &router,
        &peer(),
        "channel",
        &[7; 32],
        "signed request",
    )
    .await
    .unwrap();
    assert!(value["errors"].is_array());
    responder.await.unwrap();
    host.disconnect(generation);
}
#[tokio::test]
async fn cancellation_releases_route_and_fatal_host_errors_do_not_fall_back() {
    let host = Arc::new(Host::default());
    let router = Arc::new(Router::default());
    let (generation, mut receiver) = host.connect();
    let call = {
        let host = host.clone();
        let router = router.clone();
        tokio::spawn(async move { send(&host, &router, &peer(), "", &[7; 32], "signed").await })
    };
    let capabilities = receiver.recv().await.unwrap();
    host.reply(
        generation,
        json!({"id":capabilities["id"],"result":["LAN"]}),
    )
    .unwrap();
    let _attempt = receiver.recv().await.unwrap();
    call.abort();
    assert!(call.await.unwrap_err().is_cancelled());
    // All 64 slots remain available after cancellation.
    let mut tickets = Vec::new();
    for _ in 0..64 {
        tickets.push(
            router
                .begin(&peer(), &[TransportType::Lan])
                .unwrap()
                .ticket
                .unwrap(),
        );
    }
    for ticket in tickets {
        router.abort(&ticket);
    }
    let call = {
        let host = host.clone();
        let router = router.clone();
        tokio::spawn(async move { send(&host, &router, &peer(), "", &[7; 32], "signed").await })
    };
    let capabilities = receiver.recv().await.unwrap();
    host.reply(
        generation,
        json!({"id":capabilities["id"],"result":["LAN","BLE"]}),
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
