use super::*;
use axum::{extract::ConnectInfo, routing::get};

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
