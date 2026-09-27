use super::*;

fn pairing_event(kind: PairingEventKind) -> PairingEvent {
    PairingEvent {
        kind,
        device_id: "dev-1".into(),
        device_name: "NAS".into(),
    }
}

fn sample_request() -> crate::chat::pairing::protocol::PairingRequest {
    crate::chat::pairing::protocol::PairingRequest {
        from_id: "p".into(),
        from_name: "P".into(),
        port: 1,
        device_type: "PHONE".into(),
        ecdh_public_key: String::new(),
        signature_public_key: String::new(),
        timestamp: 0,
        ips: vec![],
        signature: String::new(),
        aware_supported: false,
        from_ip: String::new(),
    }
}

/// Pairing events map onto the phone-protocol WS numbers with the
/// plain-app payload shapes: raw `PairingRequest` for 22,
/// `DPairingResult { deviceId, deviceName, error }` for 23-26.
#[test]
fn pairing_event_ws_payload_matches_phone_protocol() {
    let (msg, payload) =
        pairing_event_ws_payload(&pairing_event(PairingEventKind::IncomingRequest {
            request: Box::new(sample_request()),
            sender_ip: String::new(),
        }))
        .unwrap();
    assert_eq!(msg, 22);
    let body: serde_json::Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(body["fromId"], "p");
    assert!(body.get("deviceId").is_none());

    let (msg, payload) =
        pairing_event_ws_payload(&pairing_event(PairingEventKind::Started)).unwrap();
    assert_eq!(msg, 26);
    let body: serde_json::Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(body["deviceId"], "dev-1");
    assert_eq!(body["deviceName"], "NAS");
    assert_eq!(body["error"], "");

    let (msg, payload) =
        pairing_event_ws_payload(&pairing_event(PairingEventKind::Success)).unwrap();
    assert_eq!(msg, 23);
    let body: serde_json::Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(body["deviceId"], "dev-1");

    let (msg, payload) =
        pairing_event_ws_payload(&pairing_event(PairingEventKind::Failed { reason: "x".into() }))
            .unwrap();
    assert_eq!(msg, 24);
    let body: serde_json::Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(body["error"], "x");

    let (msg, payload) =
        pairing_event_ws_payload(&pairing_event(PairingEventKind::Cancelled)).unwrap();
    assert_eq!(msg, 25);
    let body: serde_json::Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(body["error"], "");
}
