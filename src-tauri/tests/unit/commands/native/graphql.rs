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
