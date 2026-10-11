use super::*;
use crate::assembly::server::ContentServer;
use std::sync::Arc;
use std::time::Duration;

fn server(directory: &std::path::Path, token: &str) -> ContentServer {
    ContentServer::start(
        &directory.join("plain.db"),
        token,
        Arc::new(plain_rs::prefs::Prefs::load(&directory.join("prefs.json")).unwrap()),
    )
    .unwrap()
}

fn assert_port_released(port: u16) {
    let socket = socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::STREAM,
        Some(socket2::Protocol::TCP),
    )
    .unwrap();
    // Allow completed health probes in TIME_WAIT while still rejecting a live listener.
    socket.set_reuse_address(true).unwrap();
    socket
        .bind(&std::net::SocketAddr::from(([0, 0, 0, 0], port)).into())
        .unwrap();
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
async fn local_health_requires_auth_and_public_listeners_are_healthy() {
    let directory = tempfile::tempdir().unwrap();
    let token = plain_rs::utils::base64::base64_encode(&[9; 32]);
    let server = server(directory.path(), &token);
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/health", server.port);
    assert_eq!(client.get(&url).send().await.unwrap().status(), 401);
    assert_eq!(
        client
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert!(server.public_generation().await.is_none());
    start(&server).await;
    let state = server.runtime_state();
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
    server.stop_public().await;
    assert!(server.public_generation().await.is_none());
    server.shutdown().await;
}

#[tokio::test]
async fn listener_failure_cleans_up_and_notifies_host_with_generation() {
    let directory = tempfile::tempdir().unwrap();
    let token = plain_rs::utils::base64::base64_encode(&[10; 32]);
    let server = server(directory.path(), &token);
    let ports = start(&server).await;
    let state = server.runtime_state();
    let generation = server.public_generation().await.unwrap();
    let (host_generation, mut calls) = state.host.connect();
    let mut events = state.events.subscribe();
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
    let event = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let event = events.recv().await.unwrap();
            if event.event_type == "HTTP_SERVER_FAILED" {
                break event;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(event.event_type, "HTTP_SERVER_FAILED");
    assert!(event.is_internal());
    let failure: serde_json::Value = serde_json::from_str(&event.payload).unwrap();
    assert_eq!(failure["generation"], generation);
    assert_eq!(failure["message"], "injected listener failure");
    assert!(calls.try_recv().is_err());
    assert!(server.public_generation().await.is_none());
    for port in [ports.0, ports.1] {
        assert_port_released(port);
    }
    start(&server).await;
    assert!(state.http_failure.lock().unwrap().is_none());
    state.host.disconnect(host_generation);
    server.shutdown().await;
}

#[tokio::test]
async fn stale_failure_cannot_stop_a_new_generation() {
    let directory = tempfile::tempdir().unwrap();
    let token = plain_rs::utils::base64::base64_encode(&[11; 32]);
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
        assert_port_released(port);
    }
}

#[tokio::test]
async fn dropping_core_releases_public_listeners() {
    let directory = tempfile::tempdir().unwrap();
    let token = plain_rs::utils::base64::base64_encode(&[12; 32]);
    let server = server(directory.path(), &token);
    let ports = start(&server).await;
    drop(server);
    tokio::task::yield_now().await;
    for port in [ports.0, ports.1] {
        assert_port_released(port);
    }
}

#[tokio::test]
async fn health_and_loopback_shutdown_work_without_a_host() {
    let directory = tempfile::tempdir().unwrap();
    let token = plain_rs::utils::base64::base64_encode(&[14; 32]);
    let server = server(directory.path(), &token);
    let (port, _) = start(&server).await;
    let client = reqwest::Client::new();
    let base = format!("http://127.0.0.1:{port}");
    let response = client.get(format!("{base}/health")).send().await.unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.text().await.unwrap(), "OK");
    let response = client.get(format!("{base}/shutdown")).send().await.unwrap();
    assert_eq!(response.status(), 410);
    assert!(
        !plain_rs::prefs::user::snapshot(&server.runtime_state().prefs)
            .unwrap()
            .desktop_access
    );
    assert!(server.public_generation().await.is_some());
    assert_eq!(
        client
            .get(format!("{base}/health"))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    server.shutdown().await;
}
