use super::*;

fn sample_request() -> PairingRequest {
    PairingRequest {
        from_id: "client-a".to_string(),
        from_name: "Mac".to_string(),
        port: 8443,
        device_type: "COMPUTER".to_string(),
        ecdh_public_key: "ECDH".to_string(),
        signature_public_key: "SIGPK".to_string(),
        timestamp: 1_700_000_000_000,
        ips: vec!["203.0.113.10".to_string(), "203.0.113.11".to_string()],
        signature: String::new(),
        aware_supported: false,
        from_ip: String::new(),
    }
}

/// Lock the wire format of `PairingRequest::signature_data` — it must stay
/// byte-identical to plain-app's Kotlin canonicalization (pipe-joined, 8
/// fields, `fromIp` excluded, ips comma-joined).
#[test]
fn request_signature_data_format_is_locked() {
    let req = sample_request();
    assert_eq!(
        req.signature_data(),
        "client-a|Mac|8443|COMPUTER|ECDH|SIGPK|1700000000000|203.0.113.10,203.0.113.11"
    );
    // `from_ip` is stamped by the receiver and must not affect the signature.
    let mut stamped = req.clone();
    stamped.from_ip = "203.0.113.99".to_string();
    assert_eq!(stamped.signature_data(), req.signature_data());
    // `aware_supported` is not part of the signature either.
    let mut aware = req.clone();
    aware.aware_supported = true;
    assert_eq!(aware.signature_data(), req.signature_data());
}

/// `PairingResponse::signature_data` adds `to_id` + `accepted` (9 fields).
#[test]
fn response_signature_data_format_is_locked() {
    let resp = PairingResponse {
        from_id: "client-b".to_string(),
        to_id: "client-a".to_string(),
        port: 2443,
        device_type: "PHONE".to_string(),
        ecdh_public_key: "ECDH2".to_string(),
        signature_public_key: "SIGPK2".to_string(),
        accepted: true,
        timestamp: 1_700_000_000_500,
        ips: vec![],
        signature: String::new(),
        aware_supported: false,
    };
    assert_eq!(
        resp.signature_data(),
        "client-b|client-a|2443|PHONE|ECDH2|SIGPK2|true|1700000000500|"
    );
}

/// camelCase JSON wire names must match the Kotlin @Serializable classes.
#[test]
fn wire_json_is_camelcase() {
    let json = serde_json::to_value(sample_request()).unwrap();
    assert_eq!(json["fromId"], "client-a");
    assert_eq!(json["fromName"], "Mac");
    assert_eq!(json["deviceType"], "COMPUTER");
    assert_eq!(json["ecdhPublicKey"], "ECDH");
    assert_eq!(json["signaturePublicKey"], "SIGPK");
    assert_eq!(json["awareSupported"], false);
    assert_eq!(json["fromIp"], "");
    let back: PairingRequest = serde_json::from_value(json).unwrap();
    assert_eq!(back.from_id, "client-a");
}
