use super::*;
use crate::base64_encode;
use crate::{ed25519_generate, ed25519_sign};

/// Empty `public_key` or `signature` is accepted (permissive for
/// backward compatibility with older peers that did not sign).
/// Mirrors plain-app `DChatChannelExtensions.verifyEd25519Signature`.
#[test]
fn verify_channel_signature_permissive_on_empty() {
    let payload = "ch_x|1|invite|peer_y";
    // Both empty → accept.
    assert!(verify_channel_signature("", payload, ""));
    // Empty public key, non-empty signature → accept.
    assert!(verify_channel_signature("", payload, "AAAA"));
    // Non-empty public key, empty signature → accept.
    assert!(verify_channel_signature("AAAA", payload, ""));
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
