use super::*;
#[test]
fn cancellation_receipts_count_only_unique_requested_ids() {
    let ids = vec!["a".into(), "b".into(), "a".into()];
    assert_eq!(validate_receipt(&ids, json!(["a", "a"])).unwrap(), 1);
    assert_eq!(validate_receipt(&ids, json!([])).unwrap(), 0);
    assert!(validate_receipt(&ids, json!(["other"])).is_err());
    assert!(validate_receipt(&ids, json!(true)).is_err());
}
