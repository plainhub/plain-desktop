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
