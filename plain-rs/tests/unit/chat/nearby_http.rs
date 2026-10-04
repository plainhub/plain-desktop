use super::*;
use crate::chat::pairing::protocol::PairingCancel;
use axum::{
    Router,
    http::{HeaderMap, StatusCode},
    routing::post,
};

#[test]
fn ipv6_targets_and_canonical_pairing_wire_preserve_contract() {
    assert_eq!(url("::1", 1234).unwrap(), "https://[::1]:1234/nearby");
    assert_eq!(url("[::1]", 1234).unwrap(), "https://[::1]:1234/nearby");
    assert!(url("bad host", 1234).is_err());
    assert!(url("127.0.0.1", 0).is_err());
    let message = Message::PairCancel(PairingCancel {
        from_id: "from".into(),
        to_id: "to".into(),
    });
    assert_eq!(
        message.wire().unwrap(),
        "PAIR_CANCEL:{\"fromId\":\"from\",\"toId\":\"to\"}"
    );
}
#[cfg(feature = "http_transport")]
#[tokio::test]
async fn real_rust_generated_tls_handles_probe_pairing_rejection_timeout_and_offline() {
    let router = Router::new().route(
        "/nearby",
        post(|headers: HeaderMap, body: String| async move {
            assert_eq!(headers["content-type"], "application/json");
            if body == "DISCOVER:" {
                return StatusCode::OK;
            }
            if body == "slow" {
                tokio::time::sleep(Duration::from_millis(100)).await;
                return StatusCode::OK;
            }
            let value: serde_json::Value =
                serde_json::from_str(body.strip_prefix("PAIR_CANCEL:").unwrap()).unwrap();
            if value["toId"] == "reject" {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::OK
            }
        }),
    );
    let (cert, key) = crate::tls::generate_pem(&["localhost".into()]).unwrap();
    let server = crate::http_transport::HttpListeners::start(router, 0, 0, cert, key)
        .await
        .unwrap();
    assert!(probe("127.0.0.1", server.https_port).await.unwrap());
    let message = Message::PairCancel(PairingCancel {
        from_id: "fixture".into(),
        to_id: "accept".into(),
    });
    assert!(
        send("127.0.0.1", server.https_port, &message)
            .await
            .unwrap()
    );
    let message = Message::PairCancel(PairingCancel {
        from_id: "fixture".into(),
        to_id: "reject".into(),
    });
    assert!(
        !send("127.0.0.1", server.https_port, &message)
            .await
            .unwrap()
    );
    assert!(
        !post_url(
            &url("127.0.0.1", server.https_port).unwrap(),
            "slow".into(),
            Duration::from_millis(10)
        )
        .await
        .unwrap()
    );
    let port = server.https_port;
    server.shutdown().await;
    assert!(!probe("127.0.0.1", port).await.unwrap());
}
