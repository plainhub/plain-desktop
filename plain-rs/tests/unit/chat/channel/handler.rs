use super::*;
use crate::base64_encode;
use crate::{ed25519_generate, ed25519_sign};

#[test]
fn verify_channel_signature_rejects_missing_key_or_signature() {
    let payload = "ch_x|1|invite|peer_y";
    assert!(!verify_channel_signature("", payload, ""));
    assert!(!verify_channel_signature("", payload, "AAAA"));
    assert!(!verify_channel_signature("AAAA", payload, ""));
}

/// A real signature round-trip should verify, and tampering should fail.
#[test]
fn verify_channel_signature_roundtrip_and_tamper() {
    let (kp_bytes, vk_bytes) = ed25519_generate();
    let payload = channel_message_payload("ch_5", 4, ChannelSystemMessageAction::Kick, "peer_d");
    let sig = ed25519_sign(&kp_bytes, payload.as_bytes());
    let pub_key_b64 = base64_encode(&vk_bytes);

    assert!(
        verify_channel_signature(&pub_key_b64, &payload, &sig),
        "valid signature should verify"
    );

    let tampered = channel_message_payload("ch_5", 99, ChannelSystemMessageAction::Kick, "peer_d");
    assert!(
        !verify_channel_signature(&pub_key_b64, &tampered, &sig),
        "tampered payload should fail verification"
    );
}
