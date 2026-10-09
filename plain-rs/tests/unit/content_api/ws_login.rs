use super::*;
use crate::prefs::Prefs;

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

#[test]
fn signing_uses_the_json_keypair_the_host_stores() {
    let dir = std::env::temp_dir().join(format!(
        "plain-ws-login-sign-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let prefs = Prefs::load(&dir.join("system_prefs.json")).unwrap();
    // plain-app stores the keypair as a JSON object; Rust used to read the
    // pref as a bare string and sign with those bytes, which produced an
    // empty signature and a "signature verification failed" on the client
    // with no error on the server.
    let (keypair, public) = crate::ed25519_generate();
    prefs
        .set(
            "signature_key_pair",
            json!({
                "privateKey": crate::base64_encode(&keypair[..32]),
                "publicKey": crate::base64_encode(&public),
            })
            .to_string(),
        )
        .unwrap();

    let signature = sign(&prefs, "device|COMPLETED|pk|42").unwrap();
    assert!(
        !signature.is_empty(),
        "an empty signature looks like a valid answer and fails only on the client"
    );
    assert!(crate::crypto::ed25519_verify(
        &crate::base64_encode(&public),
        b"device|COMPLETED|pk|42",
        &signature
    ));
}

#[test]
fn signing_a_broken_keypair_is_an_error_not_an_empty_signature() {
    let dir = std::env::temp_dir().join(format!(
        "plain-ws-login-broken-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let prefs = Prefs::load(&dir.join("system_prefs.json")).unwrap();
    prefs
        .set(
            "signature_key_pair",
            json!({"privateKey": "AAAA"}).to_string(),
        )
        .unwrap();
    assert!(sign(&prefs, "device|COMPLETED|pk|42").is_err());
}
