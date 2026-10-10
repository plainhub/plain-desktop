use crate::content_api::public_gate::api_enabled;

#[test]
fn write_contact_and_call_permissions_imply_read_access() {
    let configured = ["WRITE_CONTACTS".to_owned(), "WRITE_CALL_LOG".to_owned()]
        .into_iter()
        .collect();
    assert!(api_enabled(
        &["READ_CONTACTS".into(), "WRITE_CALL_LOG".into()],
        &configured
    ));
    assert!(!api_enabled(&["READ_SMS".into()], &configured));
}

#[test]
fn every_requested_permission_must_be_configured() {
    let configured = ["READ_SMS".to_owned()].into_iter().collect();
    assert!(api_enabled(&["READ_SMS".into()], &configured));
    assert!(!api_enabled(
        &["READ_SMS".into(), "SEND_SMS".into()],
        &configured
    ));
}
