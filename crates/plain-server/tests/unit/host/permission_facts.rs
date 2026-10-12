use super::*;

#[test]
fn reports_only_existing_os_grants_and_does_not_infer_other_permissions() {
    let granted = vec!["WRITE_EXTERNAL_STORAGE".into()];
    let params = json!({"permissions":["WRITE_EXTERNAL_STORAGE","READ_CONTACTS","unknown"]});
    assert_eq!(
        facts(&granted, params.clone()).unwrap(),
        json!({"granted":{"WRITE_EXTERNAL_STORAGE":true,"READ_CONTACTS":false,"unknown":false}})
    );
    assert_eq!(
        facts(&[], params).unwrap()["granted"]["WRITE_EXTERNAL_STORAGE"],
        false
    );
    assert!(facts(&granted, json!({"permissions":[true]})).is_err());
    assert!(facts(&granted, json!({"permissions":[],"extra":true})).is_err());
}
