use super::*;
use serde_json::json;

#[test]
fn canonical_messages_round_trip_and_reject_unknown_or_malformed_payloads() {
    for value in [
        json!({"kind":"DISCOVER"}),
        json!({"kind":"PAIR_CANCEL","payload":{"fromId":"a","toId":"b"}}),
        json!({"kind":"DISCOVER_REPLY","payload":{"id":"peer","name":"Fixture","port":1234,"deviceType":"PHONE","version":"1","platform":"test"}}),
    ] {
        let message: Message = serde_json::from_value(value).unwrap();
        let expected = serde_json::to_value(&message).unwrap();
        assert_eq!(
            serde_json::to_value(Message::parse(&message.wire().unwrap()).unwrap()).unwrap(),
            expected
        );
    }
    assert_eq!(Message::Discover.wire().unwrap(), "DISCOVER:");
    for bad in [
        "UNKNOWN:{}",
        "PAIR_CANCEL:{}",
        "PAIR_CANCEL:bad",
        "no prefix",
        "PAIR_REQUEST:{}",
        "PAIR_RESPONSE:{}",
    ] {
        assert!(Message::parse(bad).is_err(), "{bad}");
    }
}
#[test]
fn discover_identity_matches_advertised_short_id_before_becoming_a_peer() {
    let body=json!({"id":"peer","name":"Fixture","port":1234,"deviceType":"PHONE","version":"1","platform":"test"}).to_string();
    assert_eq!(short_id("peer"), "2ffc1d06387ef8bb");
    assert_eq!(discover_reply(&body, &short_id("peer")).unwrap().id, "peer");
    assert!(discover_reply(&body, &short_id("other")).is_err());
    assert!(discover_reply("{}", &short_id("peer")).is_err());
}
