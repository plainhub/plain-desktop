use super::*;
use crate::{
    base64_encode,
    chat::enums::DeviceType,
    db::{DChannel, chat_store::SaveMode},
    ed25519_generate,
};
#[test]
fn signing_current_peer_and_channel_keys_and_authenticated_crypto_roundtrip() {
    let dir = std::env::temp_dir().join(format!(
        "plain-peer-wire-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let prefs = Prefs::load(&dir.join("system_prefs.json")).unwrap();
    let (key, public) = ed25519_generate();
    prefs
        .set(
            "signature_key_pair",
            json!({"privateKey":base64_encode(&key[..32]),"publicKey":base64_encode(&public)})
                .to_string(),
        )
        .unwrap();
    let db = Db::open(std::path::Path::new(":memory:")).unwrap();
    let mut peer = DPeer::new("peer", "peer", "127.0.0.1", 1, DeviceType::Phone);
    peer.key = base64_encode(&[7; 32]);
    peers::save(&db, &[peer.clone()], SaveMode::Insert).unwrap();
    let content = r#"{"type":"TEXT","value":{"text":"中文 %_ | quoted"}}"#;
    let first = prepare(
        &db,
        &prefs,
        "peer",
        Operation::Chat {
            content: content.into(),
            channel_id: String::new(),
        },
    )
    .unwrap();
    let split: Vec<_> = first.body.splitn(3, '|').collect();
    assert!(crate::ed25519_verify(
        &base64_encode(&public),
        format!("{}{}", split[1], split[2]).as_bytes(),
        split[0]
    ));
    let gql: serde_json::Value = serde_json::from_str(split[2]).unwrap();
    assert_eq!(gql["variables"]["content"], content);
    assert_eq!(first.key, peer.key);
    let encrypted = encrypt(&first.key, &first.body).unwrap();
    assert_eq!(
        decrypt(&first.key, &encrypted).unwrap().unwrap(),
        first.body
    );
    assert!(
        decrypt(&base64_encode(&[8; 32]), &encrypted)
            .unwrap()
            .is_none()
    );
    assert!(decrypt(&first.key, "plaintext").unwrap().is_none());
    let mut channel = DChannel::new("group", "owner");
    channel.key = base64_encode(&[9; 32]);
    channels::save(&db, &[channel.clone()], SaveMode::Insert).unwrap();
    let group = prepare(
        &db,
        &prefs,
        "peer",
        Operation::Chat {
            content: content.into(),
            channel_id: channel.id.clone(),
        },
    )
    .unwrap();
    assert_eq!(group.key, channel.key);
    assert_eq!(group.channel_id, channel.id);
    peer.key = base64_encode(&[4; 32]);
    peers::save(&db, &[peer.clone()], SaveMode::Update).unwrap();
    let aware = prepare(&db, &prefs, "peer", Operation::StartAware).unwrap();
    assert_eq!(aware.key, peer.key);
    assert_eq!(aware.channel_id, "");
    assert!(prepare(&db, &prefs, "missing", Operation::StartAware).is_err());
    assert!(
        prepare(
            &db,
            &prefs,
            "peer",
            Operation::Chat {
                content: content.into(),
                channel_id: "missing".into()
            }
        )
        .is_err()
    );
    assert!(encrypt("bad", "text").is_err());
    assert!(decrypt("bad", &encrypted).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}
