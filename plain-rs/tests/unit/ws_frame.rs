use super::*;

#[test]
fn named_frames_round_trip_without_numeric_type_headers() {
    let key = [7u8; 32];
    let frame = encode("PAIRING_REQUEST_RECEIVED", b"{}", &key).unwrap();
    assert!(frame.starts_with(b"PAIRING_REQUEST_RECEIVED\0"));
    assert_eq!(
        decode(&frame, &key).unwrap(),
        ("PAIRING_REQUEST_RECEIVED".into(), b"{}".to_vec())
    );
    let token = crate::base64_encode(&key);
    let frame = encode_with_token("MESSAGE_CREATED", b"[]", &token).unwrap();
    assert_eq!(
        decode_with_token(&frame, &token).unwrap(),
        ("MESSAGE_CREATED".into(), b"[]".to_vec())
    );
}

#[test]
fn raw_media_frames_keep_all_payload_bytes() {
    for kind in [
        "SCREEN_MIRROR_VIDEO",
        "SCREEN_MIRROR_AUDIO",
        "IMAGE_EDITOR_UPDATE",
    ] {
        let payload = [0, 1, 255, 0, 4];
        let frame = encode_raw(kind, &payload).unwrap();
        let (name, bytes) = decode_raw(&frame).unwrap();
        assert_eq!(name, kind);
        assert_eq!(bytes, payload);
    }
}

#[test]
fn malformed_names_numeric_frames_and_wrong_keys_are_rejected() {
    for name in [
        "",
        "pairing_started",
        "PAIRING-STARTED",
        "22",
        "MESSAGE\0CREATED",
    ] {
        assert!(encode_raw(name, b"x").is_none());
    }
    for frame in [
        b"".as_slice(),
        b"MESSAGE_CREATED",
        b"\0payload",
        b"22\0payload",
        b"message_created\0payload",
        &[0, 0, 0, 22, 1, 2],
        &[255, 0, 1],
    ] {
        assert!(decode_raw(frame).is_none());
    }
    let frame = encode("MESSAGE_CREATED", b"x", &[1; 32]).unwrap();
    assert!(decode(&frame, &[2; 32]).is_none());
    assert!(decode(b"MESSAGE_CREATED\0", &[1; 32]).is_none());
}
