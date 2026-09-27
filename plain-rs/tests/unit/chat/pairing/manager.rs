use super::*;
use crate::chat::db::ChatDb;
use crate::chat::service::ChatIdentity;
use crate::chat::transport::PeerTransport;
use std::sync::Arc;

struct TestTransport;

impl PeerTransport for TestTransport {
    fn post<'a>(
        &'a self,
        _url: &'a str,
        _client_id: &'a str,
        _channel_id: Option<&'a str>,
        _body: &'a [u8],
    ) -> impl std::future::Future<Output = Result<Vec<u8>, String>> + Send {
        std::future::ready(Ok(vec![]))
    }
}

fn manager() -> PairingManager<TestTransport> {
    let path = std::env::temp_dir().join(format!(
        "plain-rs-chat-pairing-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    let db = ChatDb::open(&path).unwrap();
    let identity = Arc::new(ChatIdentity::new("self", "NAS", ""));
    PairingManager::new(db, identity, "NAS", Arc::new(TestTransport))
}

#[test]
fn nearby_liveness_messages_are_accepted() {
    let mgr = manager();
    assert!(mgr.handle_nearby_post("DISCOVER:", "127.0.0.1"));
    assert!(mgr.handle_nearby_post("DISCOVER_REPLY:{}", "127.0.0.1"));
    assert!(!mgr.handle_nearby_post("UNKNOWN:", "127.0.0.1"));
}

#[test]
fn pair_request_dispatch_requires_valid_prefix_shape() {
    let mgr = manager();
    // Valid JSON but not a PairingRequest shape — still consumed (true),
    // no event, no crash.
    assert!(mgr.handle_nearby_post("PAIR_REQUEST:not json", "127.0.0.1"));
    // Well-formed but unsigned/stale request → no IncomingRequest event.
    let req = PairingRequest {
        from_id: "peer-a".into(),
        from_name: "Phone".into(),
        port: 8443,
        device_type: "PHONE".into(),
        ecdh_public_key: "AAAA".into(),
        signature_public_key: "BBBB".into(),
        timestamp: 0, // ancient → timestamp gate rejects
        ips: vec![],
        signature: String::new(),
        aware_supported: false,
        from_ip: String::new(),
    };
    let mut rx = mgr.subscribe();
    let body = format!("PAIR_REQUEST:{}", serde_json::to_string(&req).unwrap());
    assert!(mgr.handle_nearby_post(&body, "127.0.0.1"));
    assert!(
        rx.try_recv().is_err(),
        "stale request must not emit an event"
    );
}

#[test]
fn signed_pair_request_emits_incoming_request_with_sender_ip() {
    let mgr = manager();
    let (kp, vk) = crate::ed25519_generate();
    let ecdh = crate::EcdhSession::generate();
    let mut req = PairingRequest {
        from_id: "peer-a".into(),
        from_name: "Phone".into(),
        port: 2443,
        device_type: "PHONE".into(),
        ecdh_public_key: crate::base64_encode(&ecdh.public_key_bytes),
        signature_public_key: crate::base64_encode(&vk),
        timestamp: now_ms(),
        ips: vec![],
        signature: String::new(),
        aware_supported: false,
        from_ip: String::new(),
    };
    req.signature = crate::ed25519_sign(&kp, req.signature_data().as_bytes());

    let mut rx = mgr.subscribe();
    let body = format!("PAIR_REQUEST:{}", serde_json::to_string(&req).unwrap());
    assert!(mgr.handle_nearby_post(&body, "203.0.113.4"));

    let ev = rx.try_recv().expect("IncomingRequest event");
    assert_eq!(ev.device_id, "peer-a");
    match ev.kind {
        PairingEventKind::IncomingRequest { request, sender_ip } => {
            assert_eq!(sender_ip, "203.0.113.4");
            // The receiver stamps the sender IP into the request so the
            // frontend can pass it back to respondToPairing.
            assert_eq!(request.from_ip, "203.0.113.4");
            assert_eq!(request.from_id, "peer-a");
        }
        other => panic!("expected IncomingRequest, got {other:?}"),
    }
}

#[test]
fn cancel_pairing_without_session_is_silent() {
    let mgr = manager();
    let mut rx = mgr.subscribe();
    mgr.cancel_pairing("ghost");
    assert!(rx.try_recv().is_err());
    assert!(!mgr.is_pairing("ghost"));
}
