//! Unit tests for `src/ws_hub.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

fn k() -> [u8; crypto::KEY_LEN] {
    [3u8; crypto::KEY_LEN]
}

#[test]
fn frame_layout_matches_shared_protocol() {
    // Shared wire layout: `i32_be(msg_type) || chacha20_enc(json)`.
    // Verify the first four bytes are big-endian and decrypt cleanly
    // back to the original JSON payload.
    let payload = serde_json::json!({ "indexed": 1, "pending": 2, "total": 3, "state": "RUNNING" });
    let frame = WsHub::encode(&k(), 41, &payload).expect("encode");
    // `frame` is a `Message::Binary(Vec<u8>)`.
    let bytes = match frame {
        Message::Binary(b) => b,
        other => panic!("expected binary, got {:?}", other),
    };
    let msg_type = i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    assert_eq!(msg_type, 41);
    // Decode through the shared codec and compare the payload.
    let (t, plain) = plain_rs::ws_frame::decode(&bytes, &k()).expect("decode");
    assert_eq!(t, 41);
    let parsed: JsonValue = serde_json::from_slice(&plain).expect("json");
    assert_eq!(parsed, payload);
}

#[test]
fn chat_event_payload_frames_phone_protocol_types_verbatim() {
    // String payloads (chat WS bodies are pre-serialized) must be framed
    // byte-identically — no double JSON encoding.
    let payload = serde_json::json!({"msgType": 2, "payload": "ids=a,b"});
    let frame = WsHub::encode_chat(&k(), &payload).expect("encode");
    let Message::Binary(bytes) = frame else {
        panic!("binary")
    };
    let (t, plain) = plain_rs::ws_frame::decode(&bytes, &k()).expect("decode");
    assert_eq!(t, 2);
    assert_eq!(String::from_utf8(plain).unwrap(), "ids=a,b");

    // Object payloads (pairing events) serialize as JSON.
    let payload =
        serde_json::json!({"msgType": 22, "payload": {"kind": {"type": "incomingRequest"}}});
    let frame = WsHub::encode_chat(&k(), &payload).expect("encode");
    let Message::Binary(bytes) = frame else {
        panic!("binary")
    };
    let (t, plain) = plain_rs::ws_frame::decode(&bytes, &k()).expect("decode");
    assert_eq!(t, 22);
    let v: JsonValue = serde_json::from_slice(&plain).unwrap();
    assert_eq!(v["kind"]["type"], "incomingRequest");

    // Missing msgType → no frame (never a garbage default).
    assert!(WsHub::encode_chat(&k(), &serde_json::json!({"payload": "x"})).is_none());
}
