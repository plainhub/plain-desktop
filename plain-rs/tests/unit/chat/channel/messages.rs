use super::*;
use crate::chat::enums::{ChannelSystemMessageAction, DeviceType};
use crate::{base64_encode, ed25519_generate, ed25519_sign, ed25519_verify};

/// Verify the canonical payload format matches plain-app's
/// `channelMessagePayload`: `"$channelId|$version|$action|$target"`.
#[test]
fn payload_format_matches_android() {
    assert_eq!(
        channel_message_payload("ch_abc", 3, ChannelSystemMessageAction::Invite, "peer_xyz"),
        "ch_abc|3|INVITE|peer_xyz"
    );
    assert_eq!(
        channel_message_payload("ch_abc", 5, ChannelSystemMessageAction::Update, ""),
        "ch_abc|5|UPDATE|"
    );
    assert_eq!(
        channel_message_payload("ch_abc", 9, ChannelSystemMessageAction::Kick, ""),
        "ch_abc|9|KICK|"
    );
}

/// Round-trip: sign an `invite` payload with the owner's keypair,
/// then verify with the corresponding public key.
#[test]
fn signature_roundtrip_invite() {
    let (kp_bytes, vk_bytes) = ed25519_generate();
    let payload = channel_message_payload("ch_1", 1, ChannelSystemMessageAction::Invite, "peer_a");
    let sig = ed25519_sign(&kp_bytes, payload.as_bytes());
    assert!(!sig.is_empty(), "signature should not be empty");
    let pub_key_b64 = base64_encode(&vk_bytes);
    assert!(
        ed25519_verify(&pub_key_b64, payload.as_bytes(), &sig),
        "valid signature should verify"
    );
}

#[test]
fn signature_roundtrip_update() {
    let (kp_bytes, vk_bytes) = ed25519_generate();
    let payload = channel_message_payload("ch_2", 7, ChannelSystemMessageAction::Update, "");
    let sig = ed25519_sign(&kp_bytes, payload.as_bytes());
    let pub_key_b64 = base64_encode(&vk_bytes);
    assert!(ed25519_verify(&pub_key_b64, payload.as_bytes(), &sig));
}

#[test]
fn signature_roundtrip_kick() {
    let (kp_bytes, vk_bytes) = ed25519_generate();
    let payload = channel_message_payload("ch_3", 2, ChannelSystemMessageAction::Kick, "peer_b");
    let sig = ed25519_sign(&kp_bytes, payload.as_bytes());
    let pub_key_b64 = base64_encode(&vk_bytes);
    assert!(ed25519_verify(&pub_key_b64, payload.as_bytes(), &sig));
}

/// Tampering with the payload or signature must fail verification.
#[test]
fn signature_tamper_fails() {
    let (kp_bytes, vk_bytes) = ed25519_generate();
    let payload = channel_message_payload("ch_4", 1, ChannelSystemMessageAction::Invite, "peer_c");
    let sig = ed25519_sign(&kp_bytes, payload.as_bytes());
    let pub_key_b64 = base64_encode(&vk_bytes);

    // Tamper with version in the payload.
    let tampered =
        channel_message_payload("ch_4", 99, ChannelSystemMessageAction::Invite, "peer_c");
    assert!(
        !ed25519_verify(&pub_key_b64, tampered.as_bytes(), &sig),
        "tampered payload should fail verification"
    );

    // Tamper with the target peer id.
    let tampered_target =
        channel_message_payload("ch_4", 1, ChannelSystemMessageAction::Invite, "peer_evil");
    assert!(
        !ed25519_verify(&pub_key_b64, tampered_target.as_bytes(), &sig),
        "tampered target should fail verification"
    );

    // Garbage signature.
    let garbage_sig = base64_encode(&[0u8; 64]);
    assert!(
        !ed25519_verify(&pub_key_b64, payload.as_bytes(), &garbage_sig),
        "garbage signature should fail verification"
    );
}

/// A `ChannelInvite` round-trips through JSON with camelCase wire names
/// matching the Kotlin `@Serializable` data class.
#[test]
fn channel_invite_serializes_to_camelcase_wire() {
    let invite = ChannelInvite {
        channel_id: "ch_x".to_string(),
        channel_name: "Channel".to_string(),
        key: "a2V5".to_string(),
        owner: "owner-1".to_string(),
        members: vec![
            ChannelMember::new("owner-1"),
            ChannelMember::pending("peer-1"),
        ],
        member_peers: vec![MemberPeerInfo {
            id: "owner-1".to_string(),
            name: "Desktop".to_string(),
            public_key: "PUB".to_string(),
            device_type: DeviceType::Computer,
            ip: "".to_string(),
            port: 0,
        }],
        version: 3,
        signature: "SIG".to_string(),
    };

    let json = serde_json::to_value(&invite).expect("serialize");

    // Wire uses camelCase for multi-word fields.
    assert_eq!(json["channelId"], "ch_x");
    assert_eq!(json["channelName"], "Channel");
    assert_eq!(json["memberPeers"][0]["publicKey"], "PUB");
    assert_eq!(json["memberPeers"][0]["deviceType"], "COMPUTER");
    assert_eq!(json["members"][1]["status"], "PENDING");

    // Round-trip back to the typed struct.
    let back: ChannelInvite = serde_json::from_value(json).expect("deserialize");
    assert_eq!(back, invite);
}

/// `encode_members`/`decode_members` preserve the exact storage
/// format (`[{"peerId","status"}]` with `JOINED`/`PENDING`); legacy
/// `"id"` keys (pre-rename rows/messages) still decode via the alias.
#[test]
fn members_encode_decode_roundtrip() {
    let roster = vec![ChannelMember::new("a"), ChannelMember::pending("b")];
    let json = encode_members(&roster);
    assert_eq!(
        json,
        r#"[{"peerId":"a","status":"JOINED"},{"peerId":"b","status":"PENDING"}]"#
    );
    assert_eq!(decode_members(&json), roster);
    // Legacy storage/wire keys written by pre-rename app versions.
    assert_eq!(
        decode_members(r#"[{"id":"a","status":"JOINED"},{"id":"b","status":"PENDING"}]"#),
        roster
    );
    assert!(has_member(&roster, "a"));
    assert!(!has_member(&roster, "c"));
    assert_eq!(
        find_member(&roster, "b").map(|m| m.peer_id.as_str()),
        Some("b")
    );
}
