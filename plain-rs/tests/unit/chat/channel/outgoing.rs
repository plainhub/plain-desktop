use super::*;
use crate::{
    chat::channel::incoming,
    db::{
        DPeer,
        chat_store::{SaveMode, peers},
    },
    ed25519_generate,
};

#[test]
fn shared_invite_roundtrips_into_receiver_with_owner_key_and_preserves_real_peer_data() {
    let sender = Db::open(std::path::Path::new(":memory:")).unwrap();
    let receiver = Db::open(std::path::Path::new(":memory:")).unwrap();
    let (key, public) = ed25519_generate();
    let mut owner = DPeer::new("owner", "known", "127.0.0.1", 1, DeviceType::Phone);
    owner.public_key = base64_encode(&public);
    peers::save(&receiver, &[owner], SaveMode::Insert).unwrap();
    let guest = DPeer::new("guest", "中文 %_", "10.0.0.2", 443, DeviceType::Phone);
    peers::save(&sender, &[guest], SaveMode::Insert).unwrap();
    let mut channel = DChannel::new("group", "owner");
    channel.key = base64_encode(&[7; 32]);
    channel.members = serde_json::to_string(&vec![
        ChannelMember::new("owner"),
        ChannelMember::pending("guest"),
    ])
    .unwrap();
    let payload = build(
        Some(&sender),
        &channel,
        "owner",
        "owner name",
        DeviceType::Nas,
        &key,
        Type::Invite,
        "guest",
    )
    .unwrap();
    let msg: ChannelInvite = serde_json::from_str(&payload).unwrap();
    assert_eq!(msg.member_peers.len(), 2);
    assert_eq!(msg.member_peers[0].public_key, base64_encode(&public));
    assert_eq!(msg.member_peers[1].name, "中文 %_");
    assert!(
        incoming::receive(&receiver, "guest", "owner", Type::Invite, &payload)
            .unwrap()
            .changed
    );
    assert!(
        build(
            Some(&sender),
            &channel,
            "guest",
            "",
            DeviceType::Phone,
            &key,
            Type::Update,
            ""
        )
        .is_err()
    );
    channel.members = "corrupt".into();
    assert!(
        build(
            Some(&sender),
            &channel,
            "owner",
            "",
            DeviceType::Phone,
            &key,
            Type::Invite,
            "guest"
        )
        .is_err()
    );
}

#[test]
fn member_actions_and_directed_or_broadcast_kicks_use_canonical_wire_payloads() {
    let (key, public) = ed25519_generate();
    let channel = DChannel::new("group", "owner");
    for target in ["guest", ""] {
        let payload = build(
            None,
            &channel,
            "owner",
            "",
            DeviceType::Phone,
            &key,
            Type::Kick,
            target,
        )
        .unwrap();
        let kick: ChannelKick = serde_json::from_str(&payload).unwrap();
        assert!(crate::ed25519_verify(
            &base64_encode(&public),
            channel_message_payload(&channel.id, channel.version, Action::Kick, target).as_bytes(),
            &kick.signature
        ));
    }
    for kind in [Type::InviteAccept, Type::InviteDecline, Type::Leave] {
        let value: serde_json::Value = serde_json::from_str(
            &build(
                None,
                &channel,
                "guest",
                "中文",
                DeviceType::Phone,
                &key,
                kind,
                "owner",
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(value["channelId"], channel.id);
        if kind == Type::InviteAccept {
            assert_eq!(value["publicKey"], base64_encode(&public));
            assert_eq!(value["name"], "中文");
        }
    }
    assert!(
        build(
            None,
            &channel,
            "owner",
            "",
            DeviceType::Phone,
            &[1; 64],
            Type::Kick,
            "guest"
        )
        .is_err()
    );
}

#[tokio::test]
async fn encrypted_delivery_requires_a_true_acknowledgement() {
    struct Response(&'static str);
    impl crate::chat::transport::PeerTransport for Response {
        fn post<'a>(
            &'a self,
            _: &'a str,
            _: &'a str,
            _: Option<&'a str>,
            body: &'a [u8],
        ) -> impl std::future::Future<Output = Result<Vec<u8>, String>> + Send {
            async move {
                assert!(crate::xchacha_decrypt_raw(&[7; 32], body).is_some());
                Ok(crate::xchacha_encrypt_raw(&[7; 32], self.0.as_bytes()).unwrap())
            }
        }
    }
    let (key, _) = ed25519_generate();
    let peer = DPeer::new("guest", "guest", "127.0.0.1", 1, DeviceType::Phone);
    for (body, expected) in [
        (r#"{"data":{"channelSystemMessage":true}}"#, true),
        (r#"{"data":{"channelSystemMessage":false}}"#, false),
        (r#"{"data":{}}"#, false),
        (
            r#"{"data":{"channelSystemMessage":true},"errors":[{"message":"rejected"}]}"#,
            false,
        ),
    ] {
        assert_eq!(
            crate::chat::transport::deliver_channel_system_message(
                &Response(body),
                &peer,
                &[7; 32],
                "actor",
                &key,
                "UPDATE",
                "{}",
                None
            )
            .await,
            expected
        );
    }
}
