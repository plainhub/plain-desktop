use super::*;
use crate::chat::enums::DeviceType;
fn peer(ip: &str) -> DPeer {
    DPeer::new("fixture", "fixture", ip, 443, DeviceType::Phone)
}
fn ticket(step: Step) -> Ticket {
    step.ticket.unwrap()
}
fn failed() -> Outcome {
    Outcome::Unavailable {
        error: "offline".into(),
    }
}
#[test]
fn ordering_capabilities_ip_and_stale_receipts_are_owned_by_rust() {
    let router = Router::default();
    let now = Instant::now();
    let all = [TransportType::Ble, TransportType::Lan, TransportType::Aware];
    let lan = ticket(router.begin_at(&peer("127.0.0.1"), &all, now).unwrap());
    assert_eq!(lan.transport, TransportType::Lan);
    let aware = ticket(router.finish_at(&lan, failed(), now).unwrap());
    assert_eq!(aware.transport, TransportType::Aware);
    assert!(router.finish_at(&lan, Outcome::Connected, now).is_err());
    router.abort(&lan);
    assert_eq!(router.state.lock().unwrap().routes.len(), 1);
    let ble = ticket(router.finish_at(&aware, failed(), now).unwrap());
    assert_eq!(ble.transport, TransportType::Ble);
    assert!(
        router
            .finish_at(&ble, Outcome::Connected, now)
            .unwrap()
            .ticket
            .is_none()
    );
    assert!(router.state.lock().unwrap().routes.is_empty());
    let no_ip = ticket(router.begin_at(&peer(""), &all, now).unwrap());
    assert_eq!(no_ip.transport, TransportType::Aware);
    router.abort(&no_ip);
    let ios = ticket(
        router
            .begin_at(&peer(""), &[TransportType::Lan, TransportType::Ble], now)
            .unwrap(),
    );
    assert_eq!(ios.transport, TransportType::Ble);
    router.abort(&ios);
}
#[test]
fn two_failures_open_for_thirty_seconds_and_late_failure_cannot_undo_newer_success() {
    let router = Router::default();
    let now = Instant::now();
    let lan = [TransportType::Lan];
    let peer = peer("::1");
    for _ in 0..2 {
        let t = ticket(router.begin_at(&peer, &lan, now).unwrap());
        assert!(router.finish_at(&t, failed(), now).unwrap().error.is_some());
    }
    assert!(
        router
            .begin_at(&peer, &lan, now + Duration::from_secs(30))
            .unwrap()
            .ticket
            .is_none()
    );
    let older = ticket(
        router
            .begin_at(&peer, &lan, now + Duration::from_secs(31))
            .unwrap(),
    );
    let newer = ticket(
        router
            .begin_at(&peer, &lan, now + Duration::from_secs(31))
            .unwrap(),
    );
    router
        .finish_at(&newer, Outcome::Connected, now + Duration::from_secs(31))
        .unwrap();
    router
        .finish_at(&older, failed(), now + Duration::from_secs(31))
        .unwrap();
    let next = ticket(
        router
            .begin_at(&peer, &lan, now + Duration::from_secs(32))
            .unwrap(),
    );
    router
        .finish_at(&next, failed(), now + Duration::from_secs(32))
        .unwrap();
    assert!(
        router
            .begin_at(&peer, &lan, now + Duration::from_secs(32))
            .unwrap()
            .ticket
            .is_some()
    );
}
#[test]
fn older_success_preserves_newer_failures_and_expired_or_aborted_routes_do_not_mutate_circuits() {
    let router = Router::default();
    let now = Instant::now();
    let lan = [TransportType::Lan];
    let peer = peer("::1");
    let old = ticket(router.begin_at(&peer, &lan, now).unwrap());
    for _ in 0..2 {
        let t = ticket(router.begin_at(&peer, &lan, now).unwrap());
        router.finish_at(&t, failed(), now).unwrap();
    }
    router.finish_at(&old, Outcome::Connected, now).unwrap();
    assert!(router.begin_at(&peer, &lan, now).unwrap().ticket.is_none());
    let expired = ticket(
        router
            .begin_at(&peer, &lan, now + Duration::from_secs(31))
            .unwrap(),
    );
    assert!(
        router
            .finish_at(&expired, failed(), now + Duration::from_secs(331))
            .is_err()
    );
    let aborted = ticket(
        router
            .begin_at(&peer, &lan, now + Duration::from_secs(332))
            .unwrap(),
    );
    router.abort(&aborted);
    assert!(
        router
            .finish_at(&aborted, failed(), now + Duration::from_secs(332))
            .is_err()
    );
    assert!(
        router
            .begin_at(&peer, &lan, now + Duration::from_secs(332))
            .unwrap()
            .ticket
            .is_some()
    );
}
#[test]
fn abandoned_routes_are_bounded_and_pruned() {
    let router = Router::default();
    let now = Instant::now();
    let lan = [TransportType::Lan];
    let peer = peer("::1");
    for _ in 0..64 {
        router.begin_at(&peer, &lan, now).unwrap();
    }
    assert!(router.begin_at(&peer, &lan, now).is_err());
    assert!(
        router
            .begin_at(&peer, &lan, now + Duration::from_secs(301))
            .is_ok()
    );
    assert_eq!(router.state.lock().unwrap().routes.len(), 1);
}

#[test]
fn deleting_or_unpairing_a_peer_clears_circuits_and_invalidates_outstanding_receipts() {
    let router = Router::default();
    let now = Instant::now();
    let peer = peer("::1");
    let lan = [TransportType::Lan];
    let outstanding = ticket(router.begin_at(&peer, &lan, now).unwrap());
    for _ in 0..2 {
        let t = ticket(router.begin_at(&peer, &lan, now).unwrap());
        router.finish_at(&t, failed(), now).unwrap();
    }
    router.forget(&peer.id);
    assert!(router.finish_at(&outstanding, failed(), now).is_err());
    assert!(router.begin_at(&peer, &lan, now).unwrap().ticket.is_some());
}
