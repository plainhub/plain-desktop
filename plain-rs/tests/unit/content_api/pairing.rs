use super::*;
fn device() -> Device {
    Device {
        name: "Fixture".into(),
        port: 2443,
        device_type: "PHONE".into(),
        ips: vec!["127.0.0.1".into()],
        aware_supported: false,
    }
}
fn prefs(dir: &std::path::Path, id: &str) -> Prefs {
    let prefs = Prefs::load(&dir.join(format!("{id}.json"))).unwrap();
    let (kp, pubkey) = crate::ed25519_generate();
    prefs.set("client_id", id).unwrap();
    prefs.set("signature_key_pair",json!({"privateKey":crate::base64_encode(&kp[..32]),"publicKey":crate::base64_encode(&pubkey)}).to_string()).unwrap();
    prefs
}
#[test]
fn signed_handshake_raw_ecdh_roundtrip_and_recipient_binding() {
    let dir = std::env::temp_dir().join(format!(
        "pairing-fixture-{}-{}",
        std::process::id(),
        now_ms()
    ));
    let a = prefs(&dir, "a");
    let b = prefs(&dir, "b");
    let started = request(&a, device()).unwrap();
    let req: PairingRequest = serde_json::from_value(started["request"].clone()).unwrap();
    assert!(security::verify_request(&req));
    let built = response(&b, req.clone(), true, device()).unwrap();
    let resp: PairingResponse = serde_json::from_value(built["response"].clone()).unwrap();
    assert!(validate_response(&a, &resp, "b").unwrap());
    assert!(!validate_response(&b, &resp, "b").unwrap());
    assert!(!validate_response(&a, &resp, "c").unwrap());
    let shared = derive(
        started["privateKey"].as_str().unwrap(),
        &resp.ecdh_public_key,
    )
    .unwrap();
    assert_eq!(
        Some(shared),
        derive(built["privateKey"].as_str().unwrap(), &req.ecdh_public_key)
    );
    let raw = crate::base64_decode(started["privateKey"].as_str().unwrap());
    assert_eq!(raw.len(), 32);
    assert_eq!(
        EcdhSession::from_private_key(&raw)
            .unwrap()
            .public_key_bytes,
        crate::base64_decode(&req.ecdh_public_key)
    );
    assert!(EcdhSession::from_private_key(&[0; 32]).is_none());
    assert!(derive("invalid", &resp.ecdh_public_key).is_none());
    let rejected = response(&b, req.clone(), false, device()).unwrap();
    assert!(rejected["privateKey"].is_null());
    assert_eq!(rejected["response"]["ecdhPublicKey"], "");
    let rejected: PairingResponse = serde_json::from_value(rejected["response"].clone()).unwrap();
    assert!(validate_response(&a, &rejected, "b").unwrap());
    let mut tampered = resp.clone();
    tampered.accepted = false;
    assert!(!validate_response(&a, &tampered, "b").unwrap());
    for timestamp in [i64::MIN, i64::MAX] {
        let mut invalid = req.clone();
        invalid.timestamp = timestamp;
        assert!(!security::verify_request(&invalid));
        assert!(response(&b, invalid, true, device()).unwrap().is_null());
    }
    let mut invalid = req;
    invalid.from_name = "forged".into();
    assert!(response(&b, invalid, true, device()).unwrap().is_null());
    drop(a);
    drop(b);
    std::fs::remove_dir_all(dir).unwrap();
}
