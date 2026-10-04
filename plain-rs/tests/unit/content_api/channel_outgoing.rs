use super::*;
use crate::{base64_encode, chat::enums::PeerStatus, db::chat_store::SaveMode, ed25519_generate};
use serde_json::json;
#[test]
fn private_prepare_uses_persisted_signing_identity_and_current_peer_keys() {
    let dir = std::env::temp_dir().join(format!(
        "plain-channel-wire-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let prefs = Prefs::load(&dir.join("system_prefs.json")).unwrap();
    let (key, public) = ed25519_generate();
    prefs.set("client_id", "actor").unwrap();
    prefs
        .set(
            "signature_key_pair",
            json!({"privateKey":base64_encode(&key[..32]),"publicKey":base64_encode(&public)})
                .to_string(),
        )
        .unwrap();
    let db = Db::open(std::path::Path::new(":memory:")).unwrap();
    let mut paired = DPeer::new("paired", "paired", "127.0.0.1", 1, DeviceType::Phone);
    paired.key = base64_encode(&[8; 32]);
    paired.status = PeerStatus::Paired;
    let group = DPeer::new("group", "group", "127.0.0.1", 1, DeviceType::Phone);
    peers::save(&db, &[paired.clone(), group], SaveMode::Insert).unwrap();
    let mut channel = DChannel::new("中文 group", "actor");
    channel.key = base64_encode(&[7; 32]);
    channel.members = serde_json::to_string(&vec![
        ChannelMember::new("actor"),
        ChannelMember::new("paired"),
        ChannelMember::pending("group"),
    ])
    .unwrap();
    let result = prepare(
        &db,
        &prefs,
        &channel,
        ChannelSystemMessageType::Update,
        "",
        "name",
        DeviceType::Phone,
    )
    .unwrap();
    assert_eq!(result.targets.len(), 2);
    assert_eq!(result.targets[0].key, paired.key);
    assert_eq!(result.targets[0].channel_id, "");
    assert_eq!(result.targets[1].key, channel.key);
    assert_eq!(result.targets[1].channel_id, channel.id);
    let body = wire(&prefs, result.message_type, &result.payload).unwrap();
    let pieces: Vec<_> = body.splitn(3, '|').collect();
    assert!(crate::ed25519_verify(
        &base64_encode(&public),
        format!("{}{}", pieces[1], pieces[2]).as_bytes(),
        pieces[0]
    ));
    let wire: serde_json::Value = serde_json::from_str(pieces[2]).unwrap();
    assert_eq!(wire["variables"]["type"], "UPDATE");
    let payload: serde_json::Value =
        serde_json::from_str(wire["variables"]["payload"].as_str().unwrap()).unwrap();
    assert_eq!(payload["channelName"], channel.name);
    assert_eq!(payload["memberPeers"][0]["id"], "actor");
    assert!(
        prepare(
            &db,
            &prefs,
            &channel,
            ChannelSystemMessageType::Leave,
            "group",
            "",
            DeviceType::Phone
        )
        .is_err()
    );
    prefs
        .set(
            "signature_key_pair",
            json!({"privateKey":base64_encode(&[1;32]),"publicKey":base64_encode(&public)})
                .to_string(),
        )
        .unwrap();
    assert!(
        prepare(
            &db,
            &prefs,
            &channel,
            ChannelSystemMessageType::Update,
            "",
            "",
            DeviceType::Phone
        )
        .is_err()
    );
    std::fs::remove_dir_all(dir).unwrap();
}
