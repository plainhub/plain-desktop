use super::super::server::ContentServer;
use super::*;
use std::sync::Arc;
use std::time::Duration;

fn server(directory: &std::path::Path, token: &str) -> ContentServer {
    ContentServer::start(
        &directory.join("plain.db"),
        token,
        Arc::new(crate::prefs::Prefs::load(&directory.join("prefs.json")).unwrap()),
    )
    .unwrap()
}

async fn start(server: &ContentServer) -> (u16, u16) {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    server
        .start_public(
            0,
            0,
            cert.cert.pem().into_bytes(),
            cert.key_pair.serialize_pem().into_bytes(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn diagnostics_require_auth_and_check_both_listeners() {
    let directory = tempfile::tempdir().unwrap();
    let token = crate::base64_encode(&[9; 32]);
    let server = server(directory.path(), &token);
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/system/http-server/health", server.port);
    assert_eq!(client.post(&url).send().await.unwrap().status(), 401);
    let response: serde_json::Value = client
        .post(&url)
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .map(|body| serde_json::from_str(&body).unwrap())
        .unwrap();
    assert_eq!(response["healthy"], false);
    start(&server).await;
    let response: serde_json::Value = client
        .post(&url)
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .map(|body| serde_json::from_str(&body).unwrap())
        .unwrap();
    assert_eq!(response["healthy"], true);
    server.stop_public().await;
    let response: serde_json::Value = client
        .post(&url)
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .map(|body| serde_json::from_str(&body).unwrap())
        .unwrap();
    assert_eq!(response["healthy"], false);
    server.shutdown().await;
}

#[tokio::test]
async fn listener_failure_cleans_up_and_notifies_host_with_generation() {
    let directory = tempfile::tempdir().unwrap();
    let token = crate::base64_encode(&[10; 32]);
    let server = server(directory.path(), &token);
    let ports = start(&server).await;
    let state = server.runtime_state();
    let generation = server.public_generation().await.unwrap();
    let (host_generation, mut calls) = state.host.connect();
    let (failure, receiver) = watch::channel(None);
    let stop = state
        .public_server
        .lock()
        .await
        .as_ref()
        .unwrap()
        .stop
        .subscribe();
    monitor(state.clone(), receiver, stop, generation);
    failure
        .send(Some("injected listener failure".into()))
        .unwrap();
    let call = tokio::time::timeout(Duration::from_secs(5), calls.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(call["method"], "mainGraphqlServerFailed");
    assert_eq!(call["params"]["generation"], generation);
    assert_eq!(call["params"]["message"], "injected listener failure");
    assert!(server.public_generation().await.is_none());
    for port in [ports.0, ports.1] {
        assert!(std::net::TcpListener::bind(("0.0.0.0", port)).is_ok());
    }
    state
        .host
        .reply(
            host_generation,
            serde_json::json!({"id":call["id"], "result":null}),
        )
        .unwrap();
    state.host.disconnect(host_generation);
    server.shutdown().await;
}

#[tokio::test]
async fn stale_failure_cannot_stop_a_new_generation() {
    let directory = tempfile::tempdir().unwrap();
    let token = crate::base64_encode(&[11; 32]);
    let server = server(directory.path(), &token);
    start(&server).await;
    let old_generation = server.public_generation().await.unwrap();
    server.stop_public().await;
    let ports = start(&server).await;
    let generation = server.public_generation().await.unwrap();
    assert_ne!(old_generation, generation);
    let state = server.runtime_state();
    let (failure, receiver) = watch::channel(Some("late failure".into()));
    let stop = state
        .public_server
        .lock()
        .await
        .as_ref()
        .unwrap()
        .stop
        .subscribe();
    monitor(state.clone(), receiver, stop, old_generation);
    tokio::task::yield_now().await;
    assert_eq!(server.public_generation().await, Some(generation));
    assert!(
        state
            .public_server
            .lock()
            .await
            .as_ref()
            .unwrap()
            .listeners
            .check_health()
            .await
            .is_ok()
    );
    drop(failure);
    server.shutdown().await;
    for port in [ports.0, ports.1] {
        assert!(std::net::TcpListener::bind(("0.0.0.0", port)).is_ok());
    }
}

#[tokio::test]
async fn dropping_core_releases_public_listeners() {
    let directory = tempfile::tempdir().unwrap();
    let token = crate::base64_encode(&[12; 32]);
    let server = server(directory.path(), &token);
    let ports = start(&server).await;
    drop(server);
    tokio::task::yield_now().await;
    for port in [ports.0, ports.1] {
        assert!(std::net::TcpListener::bind(("0.0.0.0", port)).is_ok());
    }
}
