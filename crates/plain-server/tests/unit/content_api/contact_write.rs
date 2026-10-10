use super::*;

fn contact(source: &str, extra: Value) -> Value {
    let mut input = json!({
        "prefix": "",
        "firstName": "",
        "middleName": "",
        "lastName": "",
        "suffix": "",
        "nickname": "",
        "phoneNumbers": [],
        "emails": [],
        "addresses": [],
        "events": [],
        "source": source,
        "starred": false,
        "notes": "",
        "groupIds": [],
        "websites": [],
        "ims": [],
    });
    let (Value::Object(base), Value::Object(extra)) = (&mut input, extra) else {
        unreachable!()
    };
    base.extend(extra);
    input
}

#[test]
fn contact_writes_require_the_write_contacts_api_permission() {
    assert!(!permission_allowed(&[]));
    assert!(!permission_allowed(&["READ_CONTACTS".into()]));
    assert!(permission_allowed(&["WRITE_CONTACTS".into()]));
}

#[test]
fn create_requires_a_resolvable_account_source() {
    assert!(validate_contact_input(&contact("", json!({"firstName": "Ada"})), true).is_err());
    assert!(validate_contact_input(&contact("SIM", json!({"firstName": "Ada"})), true).is_ok());
    // Updating an existing contact keeps whatever account it already lives in.
    assert!(validate_contact_input(&contact("", json!({"firstName": "Ada"})), false).is_ok());
}

#[test]
fn a_contact_needs_a_name_a_phone_number_or_an_email() {
    assert!(validate_contact_input(&contact("SIM", json!({})), true).is_err());
    assert!(
        validate_contact_input(
            &contact(
                "SIM",
                json!({"phoneNumbers": [{"value": "", "type": "MOBILE", "label": ""}]})
            ),
            true
        )
        .is_err(),
        "the desktop edit form submits blank phone rows while it is being filled in"
    );
    assert!(
        validate_contact_input(
            &contact(
                "SIM",
                json!({"phoneNumbers": [{"value": "555", "type": "MOBILE", "label": ""}]})
            ),
            true
        )
        .is_ok()
    );
    assert!(
        validate_contact_input(
            &contact(
                "SIM",
                json!({"emails": [{"value": "a@b.c", "type": "HOME", "label": ""}]})
            ),
            true
        )
        .is_ok()
    );
    assert!(
        validate_contact_input(
            &contact("SIM", json!({"organization": {"company": "Bell Labs"}})),
            true
        )
        .is_ok()
    );
}

#[test]
fn oversized_and_malformed_contact_input_is_rejected() {
    let long = "x".repeat(MAX_NAME_LEN + 1);
    assert!(validate_contact_input(&contact("SIM", json!({"firstName": long})), true).is_err());
    assert!(
        validate_contact_input(
            &contact("SIM", json!({"emails": [{"value": "a@b.c", "type": 7}]})),
            true
        )
        .is_ok(),
        "the platform enum mapping stays in the host, so unknown types are not rejected here"
    );
    assert!(validate_contact_input(&contact("SIM", json!({"phoneNumbers": "555"})), true).is_err());
    assert!(
        validate_contact_input(
            &contact("SIM", json!({"phoneNumbers": [{"value": 7}]})),
            true
        )
        .is_err()
    );
    let many: Vec<Value> = (0..=MAX_ENTRIES)
        .map(|_| json!({"value": "a", "type": "HOME", "label": ""}))
        .collect();
    assert!(validate_contact_input(&contact("SIM", json!({"addresses": many})), true).is_err());
}
