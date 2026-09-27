use super::*;

#[test]
fn strip_replay_prefix_removes_timestamp_nonce_prefix() {
    assert_eq!(
        strip_replay_prefix(b"1234567890|abcdef|{\"query\":\"{x}\"}"),
        b"{\"query\":\"{x}\"}".as_slice()
    );
}

#[test]
fn strip_replay_prefix_keeps_payload_without_prefix() {
    assert_eq!(
        strip_replay_prefix(b"{\"query\":\"{x}\"}"),
        b"{\"query\":\"{x}\"}".as_slice()
    );
}

#[test]
fn strip_replay_prefix_handles_partial_prefix() {
    // One pipe only → not a replay prefix, payload kept intact.
    assert_eq!(strip_replay_prefix(b"a|b"), b"a|b".as_slice());
}
