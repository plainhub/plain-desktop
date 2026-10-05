use super::*;
#[test]
fn rejects_duplicate_stale_overflow_and_bad_format() {
    let guard = Replay::default();
    assert_eq!(
        guard
            .admit("a", "100000|nonce|{\"query\":\"a|b\"}", 100000)
            .unwrap(),
        "{\"query\":\"a|b\"}"
    );
    assert!(guard.admit("a", "100000|nonce|{}", 100000).is_err());
    assert!(guard.admit("b", "100000|nonce|{}", 100000).is_ok());
    assert!(
        guard
            .admit("a", "-9223372036854775808|x|{}", 100000)
            .is_err()
    );
    assert!(
        guard
            .admit("a", "99999999999999999999|x|{}", 100000)
            .is_err()
    );
    assert!(guard.admit("a", "39999|x|{}", 100000).is_err());
    assert!(guard.admit("a", "40000|x|{}", 100000).is_ok());
    assert!(guard.admit("a", "{}", 100000).is_err());
}
