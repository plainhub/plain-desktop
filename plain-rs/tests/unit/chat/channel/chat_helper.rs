use super::*;

fn result(peer_id: &str, error: Option<&str>) -> ChannelDeliveryResult {
    ChannelDeliveryResult {
        peer_id: peer_id.to_string(),
        peer_name: peer_id.to_string(),
        error: error.map(|e| e.to_string()),
    }
}

/// Aggregate status: no recipients / all delivered → SENT, all failed →
/// FAILED, mixed → PARTIAL. Direct translation of
/// `DMessageStatusData.aggregateStatus()`.
#[test]
fn compute_status_aggregates() {
    assert_eq!(compute_status(&[]), ChatStatus::Sent);
    assert_eq!(
        compute_status(&[result("a", None), result("b", None)]),
        ChatStatus::Sent
    );
    assert_eq!(
        compute_status(&[result("a", Some("timeout")), result("b", Some("refused"))]),
        ChatStatus::Failed
    );
    assert_eq!(
        compute_status(&[result("a", None), result("b", Some("timeout"))]),
        ChatStatus::Partial
    );
}

#[test]
fn status_data_json_shape() {
    assert_eq!(build_status_data_json(&[]), "");
    let json = build_status_data_json(&[result("a", Some("boom")), result("b", None)]);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["results"][0]["peerId"], "a");
    assert_eq!(v["results"][0]["error"], "boom");
    assert_eq!(v["results"][1]["error"], serde_json::Value::Null);
}

/// NoLeader / LeaderPeerMissing serialize `results: null` — the shape the
/// web client distinguishes from an empty result list.
#[test]
fn no_leader_status_data_is_null_results() {
    let v: serde_json::Value = serde_json::from_str(&build_no_leader_status_data()).unwrap();
    assert!(v["results"].is_null());
}
