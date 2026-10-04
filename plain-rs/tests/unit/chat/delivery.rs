use super::*;
use crate::{
    base64_encode,
    chat::{
        enums::{ChatStatus, DeviceType, PeerStatus},
        message_lifecycle,
    },
    db::{DPeer, chat_store::SaveMode},
};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicUsize, Ordering};
struct Transport {
    calls: AtomicUsize,
    db: Db,
    change_content: bool,
}
impl PeerTransport for Transport {
    async fn post<'a>(
        &'a self,
        _: &'a str,
        _: &'a str,
        _: Option<&'a str>,
        _: &'a [u8],
    ) -> std::result::Result<Vec<u8>, String> {
        unreachable!()
    }
    async fn message(
        &self,
        peer: &DPeer,
        _: &str,
        channel_id: &str,
        key: &[u8],
        body: &str,
    ) -> std::result::Result<(), String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(key, &[7; 32]);
        let graphql: Value = serde_json::from_str(body.splitn(3, '|').nth(2).unwrap()).unwrap();
        let content: Value =
            serde_json::from_str(graphql["variables"]["content"].as_str().unwrap()).unwrap();
        assert_eq!(content["type"], "TEXT");
        if self.change_content {
            self.db
                .with_conn(|c| {
                    c.execute(
                        "UPDATE chats SET content=?1 WHERE to_id=?2",
                        rusqlite::params![r#"{"type":"TEXT","value":{"text":"newer"}}"#, peer.id],
                    )
                })
                .unwrap();
        }
        if peer.id == "offline" {
            Err("offline".into())
        } else {
            assert!(channel_id.is_empty() || channel_id == "group");
            Ok(())
        }
    }
}
fn setup() -> (Db, Vec<u8>) {
    let db = Db::open(std::path::Path::new(":memory:")).unwrap();
    for id in ["peer", "offline", "unpaired"] {
        let mut peer = DPeer::new(id, id, "", 0, DeviceType::Phone);
        peer.key = base64_encode(&[7; 32]);
        peer.status = if id == "unpaired" {
            PeerStatus::Unpaired
        } else {
            PeerStatus::Paired
        };
        peers::save(&db, &[peer], SaveMode::Insert).unwrap();
    }
    (db, crate::ed25519_generate().0.to_vec())
}
const TEXT: &str = r#"{"type":"TEXT","value":{"text":"fixture"}}"#;
#[tokio::test]
async fn routing_uses_current_db_and_retries_only_current_joined_members() {
    let (db, key) = setup();
    let transport = Transport {
        calls: AtomicUsize::new(0),
        db: db.clone(),
        change_content: false,
    };
    let delivery = Delivery::new(db.clone());
    let local = message_lifecycle::create(&db, "local", "", TEXT).unwrap();
    assert_eq!(
        delivery
            .send(&transport, "self", &key, "", &local.id, None)
            .await
            .unwrap()
            .chat
            .unwrap()
            .status,
        ChatStatus::Sent
    );
    let unpaired = message_lifecycle::create(&db, "unpaired", "", TEXT).unwrap();
    assert_eq!(
        delivery
            .send(&transport, "self", &key, "", &unpaired.id, None)
            .await
            .unwrap()
            .chat
            .unwrap()
            .status,
        ChatStatus::Failed
    );
    assert_eq!(transport.calls.load(Ordering::SeqCst), 0);
    let peer = message_lifecycle::create(&db, "peer", "", TEXT).unwrap();
    assert_eq!(
        delivery
            .send(&transport, "self", &key, "", &peer.id, None)
            .await
            .unwrap()
            .chat
            .unwrap()
            .status,
        ChatStatus::Sent
    );
    let mut channel = crate::db::DChannel::new("group", "self");
    channel.id = "group".into();
    channel.key = base64_encode(&[7; 32]);
    channel.members=json!([{"peerId":"self","status":"JOINED"},{"peerId":"peer","status":"JOINED"},{"peerId":"offline","status":"JOINED"},{"peerId":"pending","status":"PENDING"}]).to_string();
    channels::save(&db, &[channel.clone()], SaveMode::Insert).unwrap();
    let group = message_lifecycle::create(&db, "", "group", TEXT).unwrap();
    let partial = delivery
        .send(&transport, "self", &key, "", &group.id, None)
        .await
        .unwrap()
        .chat
        .unwrap();
    assert_eq!(partial.status, ChatStatus::Partial);
    let count = transport.calls.load(Ordering::SeqCst);
    assert!(
        delivery
            .send(
                &transport,
                "self",
                &key,
                "",
                &group.id,
                Some(vec!["pending".into()])
            )
            .await
            .is_err()
    );
    assert!(
        delivery
            .send(
                &transport,
                "self",
                &key,
                "",
                &group.id,
                Some(vec!["peer".into(), "peer".into()])
            )
            .await
            .is_err()
    );
    assert_eq!(transport.calls.load(Ordering::SeqCst), count);
    let retry = delivery
        .send(
            &transport,
            "self",
            &key,
            "",
            &group.id,
            Some(vec!["peer".into()]),
        )
        .await
        .unwrap()
        .chat
        .unwrap();
    assert_eq!(retry.status, ChatStatus::Partial);
    assert_eq!(
        serde_json::from_str::<Value>(&retry.status_data).unwrap()["results"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(transport.calls.load(Ordering::SeqCst), count + 1);
}
#[tokio::test]
async fn changed_content_cannot_be_marked_sent_by_an_older_attempt() {
    let (db, key) = setup();
    let delivery = Delivery::new(db.clone());
    let transport = Transport {
        calls: AtomicUsize::new(0),
        db: db.clone(),
        change_content: true,
    };
    let chat = message_lifecycle::create(&db, "peer", "", TEXT).unwrap();
    assert!(
        delivery
            .send(&transport, "self", &key, "", &chat.id, None)
            .await
            .is_err()
    );
    let current = messages::get(&db, &chat.id).unwrap().unwrap();
    assert_eq!(current.status, ChatStatus::Pending);
    assert!(current.content.contains("newer"));
}
#[test]
fn attachment_uris_and_response_receipts_match_mobile_contract() {
    let token = base64_encode(&[9; 32]);
    for uri in [
        "fid:abc",
        "content://provider/123",
        "file:///tmp/a+b",
        "fsid:remote",
    ] {
        let content =
            json!({"type":"FILES","value":{"items":[{"uri":uri,"name":"x"}]}}).to_string();
        let wire = super::super::content::peer_content(&content, &token).unwrap();
        let wire: Value = serde_json::from_str(&wire).unwrap();
        let uri_wire = wire["value"]["items"][0]["uri"].as_str().unwrap();
        let decrypted = crate::xchacha_decrypt(
            &token,
            &crate::base64_decode(uri_wire.strip_prefix("fsid:").unwrap()),
        )
        .unwrap();
        assert_eq!(String::from_utf8(decrypted).unwrap(), uri);
        assert!(super::super::content::peer_content(&content, "").is_err());
    }
    assert!(
        crate::chat::transport::message_response(
            &json!({"data":{"createChatItem":[]},"errors":[]})
        )
        .is_ok()
    );
    assert!(
        crate::chat::transport::message_response(&json!({"data":{"createChatItem":null}})).is_err()
    );
    assert!(
        crate::chat::transport::message_response(
            &json!({"data":{"createChatItem":[]},"errors":[{"message":"rejected"}]})
        )
        .is_err()
    );
}

#[tokio::test]
async fn concurrent_sends_serialize_and_reload_content_after_old_attempt_finishes() {
    struct Blocking {
        entered: tokio::sync::Notify,
        release: tokio::sync::Notify,
        calls: AtomicUsize,
    }
    impl PeerTransport for Blocking {
        async fn post<'a>(
            &'a self,
            _: &'a str,
            _: &'a str,
            _: Option<&'a str>,
            _: &'a [u8],
        ) -> std::result::Result<Vec<u8>, String> {
            unreachable!()
        }
        async fn message(
            &self,
            _: &DPeer,
            _: &str,
            _: &str,
            _: &[u8],
            body: &str,
        ) -> std::result::Result<(), String> {
            if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                self.entered.notify_one();
                self.release.notified().await;
            } else {
                assert!(body.contains("newer"));
            }
            Ok(())
        }
    }
    let (db, key) = setup();
    let delivery = Arc::new(Delivery::new(db.clone()));
    let transport = Arc::new(Blocking {
        entered: tokio::sync::Notify::new(),
        release: tokio::sync::Notify::new(),
        calls: AtomicUsize::new(0),
    });
    let chat = message_lifecycle::create(&db, "peer", "", TEXT).unwrap();
    let one = {
        let d = delivery.clone();
        let t = transport.clone();
        let k = key.clone();
        let id = chat.id.clone();
        tokio::spawn(async move { d.send(&t, "self", &k, "", &id, None).await })
    };
    transport.entered.notified().await;
    messages::content(&db, &chat.id, r#"{"type":"TEXT","value":{"text":"newer"}}"#).unwrap();
    let two = {
        let d = delivery.clone();
        let t = transport.clone();
        let id = chat.id.clone();
        tokio::spawn(async move { d.send(&t, "self", &key, "", &id, None).await })
    };
    transport.release.notify_one();
    assert!(one.await.unwrap().is_err());
    let saved = two.await.unwrap().unwrap().chat.unwrap();
    assert_eq!(saved.status, ChatStatus::Sent);
    assert!(saved.content.contains("newer"));
    assert_eq!(transport.calls.load(Ordering::SeqCst), 2);
}
