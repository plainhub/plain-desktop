use super::*;
use plain_rs::crypto::EcdhSession;
use plain_rs::peers::pairing::sessions::{RESPONSE_TIMEOUT, Sessions, Target};
use std::sync::Arc;
use std::time::Duration;

fn target() -> Target {
    Target {
        device_id: "peer".into(),
        device_name: "Fixture".into(),
        device_ip: "127.0.0.1".into(),
        device_port: 1,
    }
}

/// The worker must publish one `pairing_failed` per committed expiry,
/// clear the ticket from the session store, and exit on the server's
/// stop signal rather than leaking the interval task.
#[tokio::test]
async fn timeout_worker_emits_committed_expiry_and_stops_with_server() {
    let sessions = Arc::new(Sessions::default());
    let ticket = sessions.start(target(), EcdhSession::generate());
    sessions.age_for_test("peer", RESPONSE_TIMEOUT + Duration::from_secs(1));
    let (events, mut receiver) = tokio::sync::broadcast::channel(8);
    let (stop, stopping) = tokio::sync::watch::channel(false);
    let worker = start(sessions.clone(), events, stopping);
    let event = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(event.event_type, WS_PAIRING_FAILED);
    let payload: serde_json::Value = serde_json::from_str(&event.payload).unwrap();
    assert_eq!(payload["generation"], ticket.generation);
    assert_eq!(payload["deviceId"], "peer");
    assert!(sessions.tickets().is_empty());
    assert!(sessions.take("peer").is_none());
    stop.send(true).unwrap();
    tokio::time::timeout(Duration::from_secs(2), worker)
        .await
        .unwrap()
        .unwrap();
}

/// A ticket still inside its response window must survive a worker tick.
#[tokio::test]
async fn timeout_worker_leaves_a_live_ticket_untouched() {
    let sessions = Arc::new(Sessions::default());
    sessions.start(target(), EcdhSession::generate());
    let (events, mut receiver) = tokio::sync::broadcast::channel(8);
    let (stop, stopping) = tokio::sync::watch::channel(false);
    let worker = start(sessions.clone(), events, stopping);
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert_eq!(sessions.tickets().len(), 1);
    assert!(receiver.try_recv().is_err());
    stop.send(true).unwrap();
    tokio::time::timeout(Duration::from_secs(2), worker)
        .await
        .unwrap()
        .unwrap();
}
