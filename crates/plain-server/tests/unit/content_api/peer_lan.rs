use super::*;
use crate::{
    chat::enums::{DeviceType, PeerStatus},
    content_api::ContentServer,
    db::chat_store::{SaveMode, peers},
};
fn root(id: &str) -> (tempfile::TempDir, ContentServer, Vec<u8>, String) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(crate::prefs::Prefs::load(&dir.path().join("system.json")).unwrap());
    prefs.set("client_id", id).unwrap();
    prefs.set_user("service", true).unwrap();
    let (kp, pk) = crate::ed25519_generate();
    prefs.set("signature_key_pair",json!({"privateKey":crate::base64_encode(&kp[..32]),"publicKey":crate::base64_encode(&pk)}).to_string()).unwrap();
    let token = crate::base64_encode(&[3; 32]);
    let server = ContentServer::start(&dir.path().join("plain.db"), &token, prefs).unwrap();
    (dir, server, kp.to_vec(), token)
}
fn peer(id: &str, port: u16, pk: &[u8]) -> DPeer {
    let mut peer = DPeer::new(id, id, "invalid,127.0.0.1", port, DeviceType::Phone);
    peer.status = PeerStatus::Paired;
    peer.key = crate::base64_encode(&[7; 32]);
    peer.public_key = crate::base64_encode(pk);
    peer
}
#[tokio::test]
async fn direct_tls_peer_exchange_uses_xchacha_and_current_identity_without_host() {
    let (_a, a, kp, _) = root("a");
    let (_b, b, bkp, _) = root("b");
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let (_, https) = b
        .start_public(
            0,
            0,
            cert.cert.pem().into_bytes(),
            cert.key_pair.serialize_pem().into_bytes(),
        )
        .await
        .unwrap();
    let remote = peer("b", https, &bkp[32..]);
    peers::save(&a.runtime_state().db, &[remote.clone()], SaveMode::Insert).unwrap();
    peers::save(
        &b.runtime_state().db,
        &[peer("a", 443, &kp[32..])],
        SaveMode::Insert,
    )
    .unwrap();
    let wire =
        crate::chat::transport::signed_request(&kp, "query { __typename }", json!({})).unwrap();
    let result =
        super::super::peer_transport::send(&a.runtime_state(), &remote, "", &[7; 32], &wire)
            .await
            .unwrap();
    assert_eq!(result["data"]["__typename"], "Query");
    let mut changed = remote.clone();
    changed.key = crate::base64_encode(&[9; 32]);
    peers::save(&a.runtime_state().db, &[changed], SaveMode::Update).unwrap();
    assert!(matches!(
        send(&a.runtime_state(), &remote, "", &[7; 32], &wire, || Ok(())).await,
        Err(Failure::Fatal(_))
    ));
    assert_eq!(a.runtime_state().lan.capacity.available_permits(), 4);
    a.shutdown().await;
    b.shutdown().await;
}
#[tokio::test]
async fn streaming_file_and_bad_cipher_use_real_tls_without_native_transport() {
    use axum::{
        Router,
        extract::Query,
        routing::{get, post},
    };
    let (_dir, server, kp, token) = root("a");
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let config = axum_server::tls_rustls::RustlsConfig::from_pem(
        cert.cert.pem().into_bytes(),
        cert.key_pair.serialize_pem().into_bytes(),
    )
    .await
    .unwrap();
    let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = socket.local_addr().unwrap().port();
    let endpoint = Router::new()
        .route(
            "/peer_graphql",
            post(|| async { "unauthenticated ciphertext" }),
        )
        .route(
            "/fs",
            get(
                |headers: HeaderMap,
                 Query(query): Query<std::collections::HashMap<String, String>>| async move {
                    assert_eq!(headers["c-id"], "a");
                    if query["id"] == "slow" {
                        return Body::from_stream(
                            futures_util::stream::once(async {
                                Ok::<_, std::io::Error>(bytes::Bytes::from_static(b"first"))
                            })
                            .chain(futures_util::stream::pending()),
                        );
                    }
                    assert_eq!(query["id"], " +/?#&中文%");
                    let stream = futures_util::stream::iter([
                        Ok::<_, std::io::Error>(bytes::Bytes::from_static(b"first")),
                        Ok(bytes::Bytes::from_static(b"second")),
                    ]);
                    Body::from_stream(stream)
                },
            ),
        );
    let task = tokio::spawn(
        axum_server::from_tcp_rustls(socket, config).serve(endpoint.into_make_service()),
    );
    let remote = peer("b", port, &kp[32..]);
    let state = server.runtime_state();
    peers::save(&state.db, &[remote.clone()], SaveMode::Insert).unwrap();
    assert!(matches!(
        super::send(&state, &remote, "", &[7; 32], "signed", || Ok(())).await,
        Err(Failure::Fatal(_))
    ));
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/chat/lan/file", server.port);
    let body = json!({"id":"b","expected":remote,"file_id":" +/?#&中文%"});
    assert_eq!(
        client
            .post(&url)
            .header("content-type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let response = client
        .post(&url)
        .bearer_auth(&token)
        .header("content-type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.bytes().await.unwrap().as_ref(), b"firstsecond");
    assert_eq!(state.lan.capacity.available_permits(), 4);
    let mut headers = HeaderMap::new();
    headers.insert("authorization", format!("Bearer {token}").parse().unwrap());
    let response = file(
        State(state.clone()),
        headers.clone(),
        Json(FileRequest {
            id: "b".into(),
            expected: remote.clone(),
            file_id: "slow".into(),
        }),
    )
    .await;
    assert_eq!(response.status(), 200);
    assert_eq!(state.lan.capacity.available_permits(), 3);
    drop(response);
    assert_eq!(state.lan.capacity.available_permits(), 4);
    peers::delete(&state.db, &["b".into()]).unwrap();
    assert_eq!(
        client
            .post(&url)
            .bearer_auth(&token)
            .header("content-type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .unwrap()
            .status(),
        502
    );
    peers::save(&state.db, &[remote.clone()], SaveMode::Insert).unwrap();
    let response = file(
        State(state.clone()),
        headers,
        Json(FileRequest {
            id: "b".into(),
            expected: remote,
            file_id: "slow".into(),
        }),
    )
    .await;
    assert_eq!(state.lan.capacity.available_permits(), 3);
    server.shutdown().await;
    let _ = tokio::time::timeout(
        Duration::from_secs(2),
        axum::body::to_bytes(response.into_body(), 100),
    )
    .await
    .unwrap();
    assert_eq!(state.lan.capacity.available_permits(), 4);
    task.abort();
}
