use super::*;

#[test]
fn the_login_key_is_a_truncation_of_the_same_hex_the_client_sends() {
    let digest = Sha512::digest(b"secret");
    let hex = crate::utils::hex::bytes_to_hex(&digest);
    let key = hash_to_token(&hex);
    assert_eq!(key.len(), 32);
    // The comparison is against the FULL digest hex (SHA-512 = 128 chars); the
    // key is the first 32 ASCII characters of it. Conflating the two silently
    // rejects every login.
    assert_eq!(key, hex.as_bytes()[..32].to_vec());
    assert_eq!(hex.len(), 128);
    assert_ne!(key, hex.as_bytes()[..128].to_vec());
}

#[test]
fn peer_blocks_are_only_accepted_when_usable() {
    let good = json!({"port": 8080, "signaturePublicKey": crate::utils::base64::base64_encode(&[7u8; 32])});
    assert!(peer_chat_paired(&json!({"peer": good})));
    assert!(!peer_chat_paired(&json!({})));
    assert!(!peer_chat_paired(&json!({"peer": null})));
    assert!(!peer_chat_paired(
        &json!({"peer": {"port": 0, "signaturePublicKey": ""}})
    ));
    assert!(!peer_chat_paired(
        &json!({"peer": {"port": 8080, "signaturePublicKey": "not-a-key"}})
    ));
}

#[test]
fn signature_data_matches_the_client_verification_format() {
    assert_eq!(
        signature_data("device", "COMPLETED", "pk", 42, false),
        "device|COMPLETED|pk|42"
    );
    assert_eq!(
        signature_data("device", "COMPLETED", "pk", 42, true),
        "device|COMPLETED|pk|42|true"
    );
}

#[test]
fn login_attempts_are_rate_limited_per_key() {
    let mut attempts = LoginAttempts::default();
    for _ in 0..LOGIN_ATTEMPT_LIMIT {
        assert!(attempts.acquire("10.0.0.1"));
    }
    assert!(!attempts.acquire("10.0.0.1"), "the window must lock out");
    assert!(
        attempts.acquire("10.0.0.2"),
        "another address is unaffected"
    );
}
