use super::*;
use serde_json::json;
fn payload(id: &str) -> String {
    json!({"id":id,"name":"fixture","deviceType":"PHONE","port":2443,"version":"1","platform":"android"}).to_string()
}
#[test]
fn scan_identity_generation_throttle_and_session_cleanup_are_rust_owned() {
    let scans = Scans::default();
    let session = scans.begin().unwrap();
    let short = crate::chat::nearby_wire::short_id("peer");
    let now = Instant::now();
    let Step::Read { generation } = scans.seen_at(&session, &short, now).unwrap() else {
        panic!()
    };
    assert!(matches!(
        scans.seen_at(&session, &short, now).unwrap(),
        Step::Wait
    ));
    assert!(
        scans
            .reply(&session, &short, "old", Some(&payload("peer")))
            .unwrap()
            .is_none()
    );
    assert!(
        scans
            .reply(&session, &short, &generation, Some(&payload("other")))
            .is_err()
    );
    let Step::Read { generation } = scans.seen_at(&session, &short, now).unwrap() else {
        panic!()
    };
    assert_eq!(
        scans
            .reply(&session, &short, &generation, Some(&payload("peer")))
            .unwrap()
            .unwrap()
            .id,
        "peer"
    );
    let emit = scans.0.lock().unwrap()[&session][&short].last_emit.unwrap();
    assert!(matches!(
        scans
            .seen_at(&session, &short, emit + Duration::from_secs(5))
            .unwrap(),
        Step::Wait
    ));
    assert!(matches!(
        scans
            .seen_at(&session, &short, emit + Duration::from_millis(5001))
            .unwrap(),
        Step::Emit { .. }
    ));
    scans.end(&session);
    assert!(scans.seen(&session, &short).is_err());
    assert!(
        scans
            .reply(&session, &short, &generation, Some(&payload("peer")))
            .unwrap()
            .is_none()
    );
}
#[test]
fn abandoned_gatt_reads_can_be_reclaimed_without_accepting_old_completion() {
    let scans = Scans::default();
    let id = scans.begin().unwrap();
    let short = crate::chat::nearby_wire::short_id("peer");
    let now = Instant::now();
    let Step::Read { generation: old } = scans.seen_at(&id, &short, now).unwrap() else {
        panic!()
    };
    let Step::Read { generation } = scans
        .seen_at(&id, &short, now + Duration::from_secs(30))
        .unwrap()
    else {
        panic!()
    };
    assert_ne!(old, generation);
    assert!(
        scans
            .reply(&id, &short, &old, Some(&payload("peer")))
            .unwrap()
            .is_none()
    );
    assert!(
        scans
            .reply(&id, &short, &generation, None)
            .unwrap()
            .is_none()
    );
}
