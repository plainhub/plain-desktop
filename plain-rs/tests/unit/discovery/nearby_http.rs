use super::*;
use crate::peers::pairing::protocol::PairingCancel;

#[test]
fn ipv6_targets_and_canonical_pairing_wire_preserve_contract() {
    assert_eq!(url("::1", 1234).unwrap(), "https://[::1]:1234/nearby");
    assert_eq!(url("[::1]", 1234).unwrap(), "https://[::1]:1234/nearby");
    assert!(url("bad host", 1234).is_err());
    assert!(url("127.0.0.1", 0).is_err());
    let message = Message::PairCancel(PairingCancel {
        from_id: "from".into(),
        to_id: "to".into(),
    });
    assert_eq!(
        message.wire().unwrap(),
        "PAIR_CANCEL:{\"fromId\":\"from\",\"toId\":\"to\"}"
    );
}
