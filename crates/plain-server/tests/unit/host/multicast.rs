use super::*;
use serde_json::json;

#[test]
fn closed_leases_reject_late_acquire_but_disconnect_allows_reconnection() {
    let mut leases = MulticastLeases::default();
    assert!(
        leases
            .without_platform_lock(json!({"acquire": true, "lease": "first"}))
            .unwrap()
    );
    assert!(
        leases
            .without_platform_lock(json!({"acquire": true, "lease": "second"}))
            .unwrap()
    );
    assert!(
        leases
            .without_platform_lock(json!({"acquire": false, "lease": "first"}))
            .unwrap()
    );
    assert!(
        !leases
            .without_platform_lock(json!({"acquire": true, "lease": "first"}))
            .unwrap()
    );
    assert!(leases.active.contains("second"));
    leases.disconnect();
    assert!(leases.active.is_empty());
    assert!(
        leases
            .without_platform_lock(json!({"acquire": true, "lease": "second"}))
            .unwrap()
    );
}

#[test]
fn malformed_requests_fail_and_closed_history_is_bounded() {
    let mut leases = MulticastLeases::default();
    for request in [
        json!({"acquire": true, "lease": ""}),
        json!({"acquire": "true", "lease": "first"}),
        json!({"acquire": false, "lease": "first", "extra": true}),
    ] {
        assert!(leases.without_platform_lock(request).is_err());
    }
    for id in 0..129 {
        leases
            .without_platform_lock(json!({"acquire": false, "lease": id.to_string()}))
            .unwrap();
    }
    assert_eq!(leases.closed.len(), 128);
    assert!(
        !leases
            .without_platform_lock(json!({"acquire": true, "lease": "128"}))
            .unwrap()
    );
}

#[test]
fn session_drop_clears_active_leases_without_reopening_closed_ones() {
    let leases = std::sync::Mutex::new(MulticastLeases::default());
    {
        let _session = MulticastSession::new(&leases);
        let mut state = leases.lock().unwrap();
        state
            .without_platform_lock(json!({"acquire": true, "lease": "active"}))
            .unwrap();
        state
            .without_platform_lock(json!({"acquire": false, "lease": "closed"}))
            .unwrap();
    }
    let mut state = leases.lock().unwrap();
    assert!(state.active.is_empty());
    assert!(
        !state
            .without_platform_lock(json!({"acquire": true, "lease": "closed"}))
            .unwrap()
    );
}
