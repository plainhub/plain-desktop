use super::*;
use crate::{
    base64_encode,
    chat::enums::DeviceType,
    db::{
        DPeer,
        chat_store::{SaveMode, channels, peers},
    },
    ed25519_generate, ed25519_sign,
};
use serde_json::{Value, json};

fn setup() -> (Db, Vec<u8>, String) {
    let db = Db::open(std::path::Path::new(":memory:")).unwrap();
    let (private, public) = ed25519_generate();
    let public = base64_encode(&public);
    let mut owner = DPeer::new("owner", "Owner", "127.0.0.1", 1, DeviceType::Phone);
    owner.public_key = public.clone();
    owner.key = "pairing-secret".into();
    owner.token = "login-secret".into();
    peers::save(&db, &[owner], SaveMode::Insert).unwrap();
    (db, private.to_vec(), public)
}
fn sign(key: &[u8], version: i64, action: Action, target: &str) -> String {
    ed25519_sign(
        key,
        channel_message_payload("channel", version, action, target).as_bytes(),
    )
}
fn invite(key: &[u8], public: &str, version: i64) -> Value {
    json!({"channelId":"channel","channelName":"中文 %_","owner":"owner","key":base64_encode(&[7;32]),"version":version,
        "members":[{"peerId":"owner","status":"JOINED"},{"peerId":"actor","status":"PENDING"}],
        "memberPeers":[{"id":"owner","publicKey":public,"name":"replacement"}],"signature":sign(key,version,Action::Invite,"actor")})
}
fn recv(db: &Db, actor: &str, from: &str, kind: Type, payload: &Value) -> Result<Received> {
    receive(db, actor, from, kind, &payload.to_string())
}
fn channel(db: &Db) -> DChannel {
    channels::get(db, "channel").unwrap().unwrap()
}
fn reject_writes(db: &Db) {
    db.with_conn(|c|c.execute_batch("CREATE TRIGGER reject_channel BEFORE UPDATE ON chat_channels BEGIN SELECT RAISE(ABORT,'reject fixture'); END;")).unwrap();
}

#[test]
fn invitations_require_owner_signature_and_membership_preserve_credentials_and_reject_replay() {
    let (db, key, public) = setup();
    let mut msg = invite(&key, &public, 1);
    msg["signature"] = json!("");
    assert!(recv(&db, "actor", "owner", Type::Invite, &msg).is_err());
    assert!(channels::get(&db, "channel").unwrap().is_none());
    msg = invite(&key, &public, 1);
    assert!(recv(&db, "actor", "forged", Type::Invite, &msg).is_err());
    let first = recv(&db, "actor", "owner", Type::Invite, &msg).unwrap();
    assert!(first.changed);
    assert_eq!(first.invite.unwrap().owner_peer_name, "Owner");
    let created = channel(&db);
    assert_eq!(created.status, ChannelStatus::Joined);
    assert!(
        !recv(&db, "actor", "owner", Type::Invite, &msg)
            .unwrap()
            .changed
    );
    let peer = peers::get(&db, "owner").unwrap().unwrap();
    assert_eq!(peer.key, "pairing-secret");
    assert_eq!(peer.token, "login-secret");
    assert_eq!(peer.name, "Owner");
    let kick =
        json!({"channelId":"channel","version":2,"signature":sign(&key,2,Action::Kick,"actor")});
    let result = recv(&db, "actor", "owner", Type::Kick, &kick).unwrap();
    assert!(result.cancel.is_some());
    assert_eq!(channel(&db).status, ChannelStatus::Kicked);
    assert!(recv(&db, "actor", "owner", Type::Invite, &msg).is_err());
    assert!(
        recv(
            &db,
            "actor",
            "owner",
            Type::Invite,
            &invite(&key, &public, 2)
        )
        .is_err()
    );
    msg = invite(&key, &public, 3);
    assert!(
        recv(&db, "actor", "owner", Type::Invite, &msg)
            .unwrap()
            .changed
    );
    assert_eq!(channel(&db).created_at, created.created_at);
    msg["members"] = json!([{"peerId":"owner","status":"JOINED"}]);
    assert!(recv(&db, "actor", "owner", Type::Invite, &msg).is_err());
}

#[test]
fn signed_update_rolls_back_new_peers_and_rejects_stale_or_forged_messages() {
    let (db, key, public) = setup();
    recv(
        &db,
        "actor",
        "owner",
        Type::Invite,
        &invite(&key, &public, 1),
    )
    .unwrap();
    let before = channel(&db);
    let mut msg = json!({"channelId":"channel","channelName":"after","version":2,"members":[{"peerId":"owner","status":"JOINED"},{"peerId":"actor","status":"JOINED"}],"memberPeers":[{"id":"guest","publicKey":public}],"signature":sign(&key,2,Action::Update,"")});
    reject_writes(&db);
    assert!(recv(&db, "actor", "owner", Type::Update, &msg).is_err());
    assert!(peers::get(&db, "guest").unwrap().is_none());
    assert_eq!(channel(&db).updated_at, before.updated_at);
    db.with_conn(|c| c.execute_batch("DROP TRIGGER reject_channel"))
        .unwrap();
    msg["signature"] = json!(sign(&key, 2, Action::Update, "other"));
    assert!(recv(&db, "actor", "owner", Type::Update, &msg).is_err());
    msg["signature"] = json!(sign(&key, 2, Action::Update, ""));
    assert!(
        recv(&db, "actor", "owner", Type::Update, &msg)
            .unwrap()
            .changed
    );
    assert!(peers::get(&db, "guest").unwrap().is_some());
    assert_eq!(channel(&db).name, "after");
    assert!(
        !recv(&db, "actor", "owner", Type::Update, &msg)
            .unwrap()
            .changed
    );
    let stale =
        json!({"channelId":"channel","version":1,"signature":sign(&key,1,Action::Kick,"actor")});
    assert!(
        !recv(&db, "actor", "owner", Type::Kick, &stale)
            .unwrap()
            .changed
    );
    let broadcast =
        json!({"channelId":"channel","version":2,"signature":sign(&key,2,Action::Kick,"")});
    assert!(
        recv(&db, "actor", "owner", Type::Kick, &broadcast)
            .unwrap()
            .changed
    );
    assert!(
        !recv(&db, "actor", "owner", Type::Kick, &broadcast)
            .unwrap()
            .changed
    );
}

#[test]
fn accepting_only_pending_members_is_atomic_and_does_not_create_uninvited_peers() {
    let (db, _, public) = setup();
    let mut ch = super::super::state::create(&db, "actor", "owned").unwrap();
    ch.id = "channel".into();
    ch.members = serde_json::to_string(&vec![
        ChannelMember::new("actor"),
        ChannelMember::pending("guest"),
    ])
    .unwrap();
    channels::save(&db, &[ch], SaveMode::Insert).unwrap();
    let accept =
        json!({"channelId":"channel","publicKey":public,"name":"Guest","deviceType":"PHONE"});
    assert!(
        !recv(&db, "actor", "uninvited", Type::InviteAccept, &accept)
            .unwrap()
            .changed
    );
    assert!(peers::get(&db, "uninvited").unwrap().is_none());
    reject_writes(&db);
    assert!(recv(&db, "actor", "guest", Type::InviteAccept, &accept).is_err());
    assert!(peers::get(&db, "guest").unwrap().is_none());
    assert_eq!(channel(&db).version, 1);
    db.with_conn(|c| c.execute_batch("DROP TRIGGER reject_channel"))
        .unwrap();
    let received = recv(&db, "actor", "guest", Type::InviteAccept, &accept).unwrap();
    assert!(received.broadcast);
    assert!(
        store::members(&channel(&db).members)
            .unwrap()
            .iter()
            .find(|m| m.peer_id == "guest")
            .unwrap()
            .is_joined()
    );
    assert!(
        !recv(&db, "actor", "guest", Type::InviteAccept, &accept)
            .unwrap()
            .changed
    );
    let member = json!({"channelId":"channel"});
    assert!(
        !recv(&db, "actor", "guest", Type::InviteDecline, &member)
            .unwrap()
            .changed
    );
    assert!(
        recv(&db, "actor", "guest", Type::Leave, &member)
            .unwrap()
            .broadcast
    );
    assert!(
        !recv(&db, "actor", "guest", Type::Leave, &member)
            .unwrap()
            .changed
    );
    assert_eq!(channel(&db).version, 3);
}

#[test]
fn accept_key_conflicts_and_version_overflow_preserve_peer_and_roster() {
    let (db, _, public) = setup();
    let ch = DChannel {
        id: "channel".into(),
        owner_id: "actor".into(),
        members: serde_json::to_string(&vec![
            ChannelMember::new("actor"),
            ChannelMember::pending("owner"),
        ])
        .unwrap(),
        version: i64::MAX,
        ..DChannel::new("owned", "actor")
    };
    channels::save(&db, &[ch], SaveMode::Insert).unwrap();
    let before = peers::get(&db, "owner").unwrap().unwrap();
    let mut accept = json!({"channelId":"channel","publicKey":base64_encode(&[9;32]),"name":"fake","deviceType":"PHONE"});
    assert!(recv(&db, "actor", "owner", Type::InviteAccept, &accept).is_err());
    accept["publicKey"] = json!(public);
    assert!(recv(&db, "actor", "owner", Type::InviteAccept, &accept).is_err());
    let after = peers::get(&db, "owner").unwrap().unwrap();
    assert_eq!(after.updated_at, before.updated_at);
    assert_eq!(after.token, before.token);
    assert_eq!(after.public_key, before.public_key);
    assert_eq!(channel(&db).version, i64::MAX);
    assert!(store::members(&channel(&db).members).unwrap()[1].is_pending());
}

#[test]
fn unknown_kick_requires_a_signature_and_pending_decline_is_idempotent() {
    let (db, key, _) = setup();
    let mut kick = json!({"channelId":"channel","version":1,"signature":""});
    assert!(recv(&db, "actor", "owner", Type::Kick, &kick).is_err());
    kick["signature"] = json!(sign(&key, 1, Action::Kick, "actor"));
    let effect = recv(&db, "actor", "owner", Type::Kick, &kick).unwrap();
    assert!(effect.cancel.is_some());
    assert!(!effect.changed);
    let ch = DChannel {
        id: "channel".into(),
        members: serde_json::to_string(&vec![
            ChannelMember::new("actor"),
            ChannelMember::pending("guest"),
        ])
        .unwrap(),
        ..DChannel::new("owned", "actor")
    };
    channels::save(&db, &[ch], SaveMode::Insert).unwrap();
    let decline = json!({"channelId":"channel"});
    assert!(
        recv(&db, "actor", "guest", Type::InviteDecline, &decline)
            .unwrap()
            .changed
    );
    assert!(
        !recv(&db, "actor", "guest", Type::InviteDecline, &decline)
            .unwrap()
            .changed
    );
    assert_eq!(channel(&db).version, 2);
    assert!(peers::get(&db, "guest").unwrap().is_none());
}
