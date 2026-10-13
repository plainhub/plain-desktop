use super::*;
use crate::graphql::executor::content_changes;

#[tokio::test]
async fn app_and_remote_access_share_listeners_without_background_service() {
    let directory = tempfile::tempdir().unwrap();
    let prefs =
        Arc::new(plain_rs::prefs::Prefs::load(&directory.path().join("prefs.json")).unwrap());
    prefs.set_user("service", false).unwrap();
    prefs.set_user("desktop_access", false).unwrap();
    let token = plain_rs::utils::base64::base64_encode(&[19; 32]);
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let server = ContentServer::start_http(
        &directory.path().join("plain.db"),
        &token,
        prefs.clone(),
        0,
        0,
        cert.cert.pem().into_bytes(),
        cert.key_pair.serialize_pem().into_bytes(),
        false,
        "",
    )
    .await
    .unwrap();
    assert!(
        server.task.is_none(),
        "unified mode must not create a separate loopback listener"
    );
    let (http, https) = server.http_ports().await.unwrap();
    assert_eq!(http, server.port);
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();
    for url in [
        format!("http://127.0.0.1:{http}"),
        format!("https://127.0.0.1:{https}"),
    ] {
        let response = client
            .get(format!("{url}/health"))
            .bearer_auth(&token)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(
            client
                .post(format!("{url}/graphql"))
                .bearer_auth(&token)
                .header("content-type", "application/json")
                .body(serde_json::json!({"query":"{ __typename }"}).to_string())
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
        assert_eq!(
            client
                .post(format!("{url}/graphql"))
                .header("content-type", "application/json")
                .body(serde_json::json!({"query":"{ __typename }"}).to_string())
                .send()
                .await
                .unwrap()
                .status(),
            404
        );
        assert_eq!(
            client
                .get(format!("{url}/host"))
                .send()
                .await
                .unwrap()
                .status(),
            404
        );
    }
    let router = unified_router(
        server.state.clone(),
        server.local_router.as_ref().unwrap().clone(),
        Router::new(),
    );
    use tower::ServiceExt;
    for peer in [None, Some("192.0.2.42:1234"), Some("127.0.0.1:1234")] {
        let mut request = axum::http::Request::builder()
            .method("GET")
            .uri("/health")
            .header("authorization", format!("Bearer {token}"))
            .header("x-forwarded-for", "127.0.0.1")
            .body(axum::body::Body::empty())
            .unwrap();
        if let Some(peer) = peer {
            request.extensions_mut().insert(axum::extract::ConnectInfo(
                peer.parse::<std::net::SocketAddr>().unwrap(),
            ));
        }
        let expected = if peer == Some("127.0.0.1:1234") {
            200
        } else {
            403
        };
        assert_eq!(
            router.clone().oneshot(request).await.unwrap().status(),
            expected
        );
    }
    prefs.set_user("desktop_access", true).unwrap();
    assert_eq!(
        client
            .post(format!("http://127.0.0.1:{http}/graphql"))
            .header("content-type", "application/json")
            .body(serde_json::json!({"query":"{ __typename }"}).to_string())
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    server.shutdown().await;
}

#[tokio::test]
async fn configured_public_uses_core_settings_and_keeps_listeners_on_tls_preflight_error() {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    assert!(core.set_public_ports(Some(0), Some(0)).await.unwrap());
    assert_eq!(
        core.runtime_state()
            .prefs
            .get_user::<i32>("http_port")
            .unwrap(),
        Some(0)
    );
    assert_eq!(
        core.runtime_state()
            .prefs
            .get_user::<i32>("https_port")
            .unwrap(),
        Some(0)
    );
    let ports = core.start_configured_public().await.unwrap();
    assert_ne!(ports.0, 0);
    assert_ne!(ports.1, 0);
    assert_eq!(core.http_ports().await, Some(ports));
    assert!(core.task.is_none());
    std::fs::write(directory.path().join("tls-identity.json"), "{}").unwrap();
    assert!(core.restart_public().await.is_err());
    assert_eq!(core.http_ports().await, Some(ports));
    core.stop_core().await;
    assert_eq!(core.http_ports().await, None);
    assert!(core.native_url_token().is_err());
    assert!(core.set_public_ports(Some(8080), None).await.is_err());
    assert!(core.start_configured_public().await.is_err());
    assert!(core.restart_public().await.is_err());
}

#[cfg(feature = "core")]
#[tokio::test]
async fn shell_preferences_share_the_core_owner() {
    let directory = tempfile::tempdir().unwrap();
    let core = ContentServer::open(directory.path()).unwrap();
    let shell = core.shared_preferences().unwrap();
    assert!(Arc::ptr_eq(&shell, &core.state.prefs));
    shell.set_user("device_name", "Shared device").unwrap();
    assert_eq!(
        core.preferences()
            .unwrap()
            .get_user::<String>("device_name")
            .unwrap(),
        Some("Shared device".into())
    );
    core.stop_core().await;
    assert!(core.shared_preferences().is_err());
}
