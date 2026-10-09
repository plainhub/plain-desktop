use super::*;

#[tokio::test]
async fn app_and_remote_access_share_listeners_without_background_service() {
    let directory = tempfile::tempdir().unwrap();
    let prefs = Arc::new(crate::prefs::Prefs::load(&directory.path().join("prefs.json")).unwrap());
    prefs.set_user("service", false).unwrap();
    prefs.set_user("desktop_access", false).unwrap();
    let token = crate::base64_encode(&[19; 32]);
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
            .post(format!("{url}/system/http-server/health"))
            .bearer_auth(&token)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body: serde_json::Value =
            serde_json::from_slice(&response.bytes().await.unwrap()).unwrap();
        assert_eq!(body["healthy"], true);
        assert_eq!(
            client
                .post(format!("{url}/graphql"))
                .bearer_auth(&token)
                .header("content-type", "application/json").body(serde_json::json!({"query":"{ __typename }"}).to_string())
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
        assert_eq!(
            client
                .post(format!("{url}/graphql"))
                .header("content-type", "application/json").body(serde_json::json!({"query":"{ __typename }"}).to_string())
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
            .method("POST")
            .uri("/system/http-server/health")
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
            .header("content-type", "application/json").body(serde_json::json!({"query":"{ __typename }"}).to_string())
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    server.shutdown().await;
}
