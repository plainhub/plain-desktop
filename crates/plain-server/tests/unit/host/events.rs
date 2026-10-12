use super::*;
use crate::assembly::server::ContentServer;
use std::{sync::Arc, time::Duration};

fn server(dir: &tempfile::TempDir) -> ContentServer {
    ContentServer::start(
        &dir.path().join("db"),
        &plain_rs::utils::base64::base64_encode(&[9; 32]),
        Arc::new(plain_rs::prefs::Prefs::load(&dir.path().join("prefs.json")).unwrap()),
    )
    .unwrap()
}
#[tokio::test]
async fn native_events_enforce_direction_and_reconcile_lag() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(&dir);
    let bridge = server.host_events();
    let mut public = server.runtime_state().events.subscribe();
    for kind in [
        "MESSAGE_CREATED",
        "PAIRING_SUCCESS",
        "CONTENT_CHANGED",
        "UNKNOWN_EVENT",
        plain_rs::events::HTTP_SERVER_FAILED,
        plain_rs::events::MEDIA_ACTION_FINISHED,
    ] {
        assert!(bridge.publish_text(kind, "{}".into()).is_err());
        assert!(bridge.publish_binary(kind, vec![]).is_err());
    }
    for kind in HOST_TEXT_TYPES {
        bridge.publish_text(kind, "{}".into()).unwrap();
        assert!(public.recv().await.unwrap().is_host_event());
    }
    for kind in HOST_BINARY_TYPES {
        bridge.publish_binary(kind, vec![1, 2, 3]).unwrap();
        assert_eq!(
            public.recv().await.unwrap().binary_payload,
            Some(vec![1, 2, 3])
        );
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(20), bridge.next_event())
            .await
            .is_err()
    );
    server
        .runtime_state()
        .events
        .send(WsEvent::broadcast("PAIRING_SUCCESS", "{}".into()))
        .unwrap();
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "PAIRING_SUCCESS"
    );
    for _ in 0..300 {
        server
            .runtime_state()
            .events
            .send(WsEvent::broadcast("CONTENT_CHANGED", "{}".into()))
            .unwrap();
    }
    assert_eq!(bridge.next_event().await.unwrap()["reconcile"], true);
    server.shutdown().await;
    assert!(bridge.next_event().await.is_none());
}
#[tokio::test]
async fn native_requests_reply_and_replacement_cancel_pending_calls() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(&dir);
    let bridge = server.host_events();
    let host = server.runtime_state().host.clone();
    let call_host = host.clone();
    let task = tokio::spawn(async move { call_host.call("fixture", json!({})).await });
    let request = bridge.next_request().await.unwrap();
    bridge
        .reply(json!({"id":request["id"],"result":42}))
        .unwrap();
    assert_eq!(task.await.unwrap().unwrap(), json!(42));
    let task = tokio::spawn(async move { host.call("fixture", json!({})).await });
    bridge.next_request().await.unwrap();
    let replacement = server.host_events();
    assert!(task.await.unwrap().unwrap_err().contains("replaced"));
    assert!(
        bridge
            .publish_text(HOST_TEXT_TYPES[0], "{}".into())
            .is_err()
    );
    drop(bridge);
    assert!(
        server
            .runtime_state()
            .host
            .is_current(replacement.generation)
    );
    server.shutdown().await;
}

#[cfg(feature = "http_transport")]
#[tokio::test]
async fn listener_failure_survives_event_queue_overflow() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(&dir);
    let bridge = server.host_events();
    let state = server.runtime_state();
    *state.http_failure.lock().unwrap() = Some(crate::assembly::lifecycle::HttpServerFailure {
        generation: 7,
        message: "failed".into(),
    });
    for _ in 0..300 {
        let _ = state
            .events
            .send(WsEvent::broadcast("CONTENT_CHANGED", "{}".into()));
    }
    let event = bridge.next_event().await.unwrap();
    assert_eq!(event["type"], "HTTP_SERVER_FAILED");
    assert_eq!(event["reconcile"], true);
    let payload: Value = serde_json::from_str(event["payload"].as_str().unwrap()).unwrap();
    assert_eq!(payload["generation"], 7);
    server.shutdown().await;
}

#[tokio::test]
async fn replacing_a_host_wakes_a_waiting_event_receiver() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(&dir);
    let original = server.host_events();
    let mut waiting = Box::pin(original.next_event());
    std::future::poll_fn(|context| {
        assert!(std::future::Future::poll(waiting.as_mut(), context).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    let replacement = server.host_events();
    std::future::poll_fn(|context| {
        assert!(matches!(
            std::future::Future::poll(waiting.as_mut(), context),
            std::task::Poll::Ready(None)
        ));
        std::task::Poll::Ready(())
    })
    .await;
    drop(waiting);
    drop(original);
    assert!(
        server
            .runtime_state()
            .host
            .is_current(replacement.generation)
    );
    server.shutdown().await;
}

#[tokio::test]
async fn replaced_hosts_discard_requests_and_events_already_in_the_queue() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(&dir);
    let original = server.host_events();
    server
        .runtime_state()
        .events
        .send(WsEvent::broadcast("CONTENT_CHANGED", "{}".into()))
        .unwrap();
    let replacement = server.host_events();
    assert!(original.next_request().await.is_none());
    assert!(original.next_event().await.is_none());
    original.wait_disconnected().await;
    assert!(
        server
            .runtime_state()
            .host
            .is_current(replacement.generation)
    );
    server.shutdown().await;
}
