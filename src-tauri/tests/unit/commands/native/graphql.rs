use super::*;
use serde_json::json;

#[test]
fn deadlines_reject_delayed_requests_and_preserve_remaining_time() {
    let body = json!({"extensions":{"nativeDeadlineMs":1500}});
    assert_eq!(
        request_deadline(&body, 1000).unwrap(),
        Duration::from_millis(500)
    );
    assert_eq!(
        request_deadline(&body, 1500).unwrap_err(),
        "connection_timeout"
    );
    assert_eq!(
        request_deadline(&body, 1600).unwrap_err(),
        "connection_timeout"
    );
    assert!(request_deadline(&json!({}), 1000).is_err());
    assert!(request_deadline(&json!({"extensions":{"nativeDeadlineMs":-1}}), 1000).is_err());
}

#[test]
fn graphql_targets_accept_only_local_or_peer_identity() {
    assert_eq!(
        serde_json::from_value::<NativeTarget>(json!({"kind":"local"})).unwrap(),
        NativeTarget::Local {}
    );
    assert_eq!(
        serde_json::from_value::<NativeTarget>(json!({"kind":"peer","peerId":"paired"})).unwrap(),
        NativeTarget::Peer {
            peer_id: "paired".into()
        }
    );
    for target in [
        json!({"kind":"peer"}),
        json!({"kind":"peer","peerId":"paired","url":"https://other.invalid"}),
        json!({"kind":"peer","peerId":"paired","token":"secret"}),
        json!({"kind":"local","peerId":"paired"}),
        json!({"kind":"proxy","peerId":"paired","via":"other"}),
    ] {
        assert!(
            serde_json::from_value::<NativeTarget>(target.clone()).is_err(),
            "{target}"
        );
    }
}

#[test]
fn graphql_scope_cannot_be_rebound_to_another_window() {
    let views = crate::commands::native::views::NativeViews::default();
    let source = views.scope("main");
    views.scope("other");
    assert!(views.existing_scope("main", source.id()).is_ok());
    assert!(views.existing_scope("other", source.id()).is_err());
    source.close();
    assert!(views.existing_scope("main", source.id()).is_err());
}
