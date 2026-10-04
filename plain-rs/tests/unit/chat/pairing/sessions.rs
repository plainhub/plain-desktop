use super::*;
fn target() -> Target {
    Target {
        device_id: "peer".into(),
        device_name: "Fixture".into(),
        device_ip: "127.0.0.1".into(),
        device_port: 1,
    }
}
#[test]
fn old_timer_and_failure_cleanup_cannot_remove_replacement_session() {
    let sessions = Sessions::default();
    let old = sessions.start(target(), EcdhSession::generate());
    let next = sessions.start(target(), EcdhSession::generate());
    assert!(sessions.cancel("peer", Some(&old.generation)).is_none());
    assert!(sessions.expire("peer", &old.generation).is_none());
    assert!(sessions.expire("peer", &next.generation).is_none());
    sessions.0.lock().unwrap().get_mut("peer").unwrap().started = Instant::now() - RESPONSE_TIMEOUT;
    assert_eq!(
        sessions
            .expire("peer", &next.generation)
            .unwrap()
            .generation,
        next.generation
    );
    assert!(sessions.take("peer").is_none());
}
#[test]
fn concurrent_responses_consume_ephemeral_key_once() {
    let sessions = std::sync::Arc::new(Sessions::default());
    sessions.start(target(), EcdhSession::generate());
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let sessions = sessions.clone();
            std::thread::spawn(move || sessions.take("peer").is_some())
        })
        .collect();
    assert_eq!(
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .filter(|won| *won)
            .count(),
        1
    );
}

#[test]
fn batch_expiry_keeps_fresh_replacements_and_returns_each_expired_ticket_once() {
    let sessions = Sessions::default();
    let old = sessions.start(target(), EcdhSession::generate());
    sessions.0.lock().unwrap().get_mut("peer").unwrap().started = Instant::now() - RESPONSE_TIMEOUT;
    let fresh = sessions.start(target(), EcdhSession::generate());
    assert!(sessions.expire_all().is_empty());
    assert_eq!(sessions.tickets()[0].generation, fresh.generation);
    assert_ne!(old.generation, fresh.generation);
    sessions.0.lock().unwrap().get_mut("peer").unwrap().started = Instant::now() - RESPONSE_TIMEOUT;
    assert!(sessions.tickets().is_empty());
    assert_eq!(sessions.expire_all()[0].generation, fresh.generation);
    assert!(sessions.expire_all().is_empty());
}

#[cfg(feature = "content_api")]
#[tokio::test]
async fn rust_timeout_worker_emits_committed_expiry_and_stops_with_server() {
    let sessions = std::sync::Arc::new(Sessions::default());
    let ticket = sessions.start(target(), EcdhSession::generate());
    sessions.0.lock().unwrap().get_mut("peer").unwrap().started = Instant::now() - RESPONSE_TIMEOUT;
    let (events, mut receiver) = tokio::sync::broadcast::channel(8);
    let (stop, stopping) = tokio::sync::watch::channel(false);
    let worker = crate::content_api::pairing_timeout::start(sessions.clone(), events, stopping);
    let event = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(event.event_type, crate::chat::events::WS_PAIRING_FAILED);
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
