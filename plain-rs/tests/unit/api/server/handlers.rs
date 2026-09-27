use super::*;

#[test]
fn strip_replay_wrapper_removes_timestamp_nonce_prefix() {
    assert_eq!(
        strip_replay_wrapper(b"1234567890|abcdef|{\"query\":\"{x}\"}"),
        "{\"query\":\"{x}\"}"
    );
}

#[test]
fn strip_replay_wrapper_keeps_payload_without_prefix() {
    assert_eq!(
        strip_replay_wrapper(b"{\"query\":\"{x}\"}"),
        "{\"query\":\"{x}\"}"
    );
}

#[test]
fn strip_replay_wrapper_handles_partial_prefix() {
    // One pipe only, or a non-numeric timestamp → not a replay prefix,
    // payload kept intact.
    assert_eq!(strip_replay_wrapper(b"a|b"), "a|b");
    assert_eq!(strip_replay_wrapper(b"abc|def|ghi"), "abc|def|ghi");
    assert_eq!(strip_replay_wrapper(b"1234567890|onlyone"), "1234567890|onlyone");
}
