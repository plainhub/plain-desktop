use super::*;
use axum::{extract::ConnectInfo, routing::get, routing::post};

fn certificate() -> (Vec<u8>, Vec<u8>) {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    (
        cert.cert.pem().into_bytes(),
        cert.key_pair.serialize_pem().into_bytes(),
    )
}

#[tokio::test]
async fn serves_http_and_tls_then_releases_both_ports() {
    let (cert, key) = certificate();
    let router = Router::new().route(
        "/health",
        get(|ConnectInfo(addr): ConnectInfo<SocketAddr>| async move { addr.ip().to_string() }),
    );
    let server = HttpListeners::start(router, 0, 0, cert, key).await.unwrap();
    let http = server.http_port;
    let https = server.https_port;
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();
    for scheme_port in [("http", http), ("https", https)] {
        let response = client
            .get(format!(
                "{}://127.0.0.1:{}/health",
                scheme_port.0, scheme_port.1
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.text().await.unwrap(), "127.0.0.1");
    }
    server.shutdown().await;
    assert!(std::net::TcpListener::bind(("0.0.0.0", http)).is_ok());
    assert!(std::net::TcpListener::bind(("0.0.0.0", https)).is_ok());
}

#[tokio::test]
async fn invalid_tls_never_opens_http_listener() {
    let probe = std::net::TcpListener::bind("0.0.0.0:0").unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);
    let result = HttpListeners::start(Router::new(), port, 0, vec![], vec![]).await;
    assert!(result.is_err());
    assert!(std::net::TcpListener::bind(("0.0.0.0", port)).is_ok());
}

#[tokio::test]
async fn drop_releases_listeners() {
    let (cert, key) = certificate();
    let server = HttpListeners::start(Router::new(), 0, 0, cert, key)
        .await
        .unwrap();
    let ports = [server.http_port, server.https_port];
    drop(server);
    tokio::task::yield_now().await;
    for port in ports {
        assert!(std::net::TcpListener::bind(("0.0.0.0", port)).is_ok());
    }
}

#[tokio::test]
async fn listener_task_abort_is_reported_and_shutdown_releases_ports() {
    let (cert, key) = certificate();
    let server = HttpListeners::start(Router::new(), 0, 0, cert, key)
        .await
        .unwrap();
    let ports = [server.http_port, server.https_port];
    let mut failure = server.failures();
    server.tasks[1].abort();
    tokio::time::timeout(Duration::from_secs(1), failure.changed())
        .await
        .unwrap()
        .unwrap();
    assert!(failure.borrow().as_ref().unwrap().contains("HTTPS"));
    assert!(server.check_health().await.is_err());
    server.shutdown().await;
    for port in ports {
        assert!(std::net::TcpListener::bind(("0.0.0.0", port)).is_ok());
    }
}

/// The nearby wire speaks over the generated-TLS listener: plain-rs's
/// client must probe, deliver a pairing cancel, read a rejection as
/// `false`, honour its own deadline, and fail once the listener is gone.
#[tokio::test]
async fn nearby_client_handles_probe_accept_reject_timeout_and_offline() {
    use axum::http::{HeaderMap, StatusCode};
    use plain_rs::discovery::nearby_http::{Message, post_url, probe, send, url};
    use plain_rs::peers::pairing::protocol::PairingCancel;

    let (cert, key) = certificate();
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
    let server = HttpListeners::start(router, 0, 0, cert, key).await.unwrap();
    assert!(probe("127.0.0.1", server.https_port).await.unwrap());
    let cancel = |to: &str| {
        Message::PairCancel(PairingCancel {
            from_id: "fixture".into(),
            to_id: to.into(),
        })
    };
    assert!(
        send("127.0.0.1", server.https_port, &cancel("accept"))
            .await
            .unwrap()
    );
    assert!(
        !send("127.0.0.1", server.https_port, &cancel("reject"))
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

#[tokio::test]
async fn occupied_ports_fall_back_for_both_listeners() {
    let occupied_http = std::net::TcpListener::bind("0.0.0.0:0").unwrap();
    let occupied_https = std::net::TcpListener::bind("0.0.0.0:0").unwrap();
    let http = occupied_http.local_addr().unwrap().port();
    let https = occupied_https.local_addr().unwrap().port();
    let (cert, key) = certificate();
    let listeners = HttpListeners::start(Router::new(), http, https, cert, key)
        .await
        .unwrap();
    assert_ne!(listeners.http_port, http);
    assert_ne!(listeners.https_port, https);
    listeners.check_health().await.unwrap();
    listeners.shutdown().await;
}
