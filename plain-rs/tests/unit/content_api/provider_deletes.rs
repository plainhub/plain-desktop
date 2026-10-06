use super::*;

#[test]
fn provider_deletes_are_gated_on_their_own_write_permission() {
    assert_eq!(
        delete_permission(&Provider::Contact),
        Some("WRITE_CONTACTS")
    );
    assert_eq!(delete_permission(&Provider::Call), Some("WRITE_CALL_LOG"));
    assert_eq!(delete_permission(&Provider::File), None);
}
