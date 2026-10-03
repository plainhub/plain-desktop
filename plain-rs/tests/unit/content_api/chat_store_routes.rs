use super::*;
use crate::{
    chat::enums::{ChatStatus, DeviceType, PeerStatus},
    content_api::ContentServer,
    db::{DChannel, DChat, DNearbyDeviceCache, DPeer, Db},
};
use chat_store::{channels, messages, nearby, peers};
use std::sync::Arc;
#[test]
fn checked_chat_storage_is_atomic_literal_and_preserves_unrelated_conversations_and_login_tokens() {
    let db = Db::open(std::path::Path::new(":memory:")).unwrap();
    let mut peer = DPeer::new("peer", "name", "127.0.0.1", 443, DeviceType::Phone);
    peer.token = "session".into();
    peers::save(&db, std::slice::from_ref(&peer), SaveMode::Insert).unwrap();
    peer.name = "renamed".into();
    peer.token.clear();
    peers::save(&db, std::slice::from_ref(&peer), SaveMode::Update).unwrap();
    assert_eq!(peers::get(&db, "peer").unwrap().unwrap().token, "session");
    let before = peers::get(&db, "peer").unwrap().unwrap();
    let mut after = before.clone();
    after.ip = "::1".into();
    db.with_conn(|c| c.execute("UPDATE peers SET key='new-key' WHERE id='peer'", []))
        .unwrap();
    assert_eq!(
        peers::patch(&db, &before, &after).unwrap().unwrap().key,
        "new-key"
    );
    let missing = DPeer::new("missing", "", "", 0, DeviceType::Unknown);
    peer.name = "must rollback".into();
    assert!(peers::save(&db, &[peer.clone(), missing], SaveMode::Update).is_err());
    assert_eq!(peers::get(&db, "peer").unwrap().unwrap().name, "renamed");
    let mut channel = DChannel::new("group", "me");
    channel.id = "channel".into();
    channel.version = 5_000_000_001;
    channel.members = r#"[{"peerId":"peer","status":"JOINED"}]"#.into();
    channels::save(&db, &[channel.clone()], SaveMode::Insert).unwrap();
    assert_eq!(
        channels::get(&db, "channel").unwrap().unwrap().version,
        5_000_000_001
    );
    let mut newer = channel.clone();
    newer.version += 1;
    channels::patch(&db, &channel, &newer).unwrap();
    assert!(channels::patch(&db, &channel, &channel).is_err());

    let mut one = DChat::new(
        "me",
        "peer",
        "",
        r#"{"type":"TEXT","value":{"text":"literal %_\\' 中文"}}"#,
    );
    one.id = "one".into();
    one.created_at = "2026-10-04T00:00:00Z".into();
    let mut two = DChat::new(
        "peer",
        "me",
        "",
        r#"{"type":"TEXT","value":{"text":"second"}}"#,
    );
    two.id = "two".into();
    two.created_at = "2026-10-04T00:00:01Z".into();
    let mut group = DChat::new(
        "peer",
        "me",
        "channel",
        r#"{"type":"TEXT","value":{"text":"group"}}"#,
    );
    group.id = "group".into();
    messages::save(&db, &[one.clone(), two.clone(), group], SaveMode::Insert).unwrap();
    let mut filter = messages::Filter {
        peer: Some("peer".into()),
        channel: None,
        text: String::new(),
        offset: 0,
        limit: Some(1),
        descending: true,
        latest: false,
        count_only: false,
    };
    let rows: Vec<DChat> = serde_json::from_value(messages::list(&db, &filter).unwrap()).unwrap();
    assert_eq!(rows[0].id, "two");
    filter.limit = None;
    filter.text = "%_".into();
    filter.count_only = true;
    assert_eq!(messages::list(&db, &filter).unwrap(), json!(1));
    filter.text = "%' OR 1=1 --".into();
    assert_eq!(messages::list(&db, &filter).unwrap(), json!(0));
    assert_eq!(messages::ids(&db, "peer:peer").unwrap().len(), 2);
    assert_eq!(
        messages::ids(&db, "channel:channel").unwrap(),
        vec!["group"]
    );
    assert!(messages::ids(&db, "text:unknown").unwrap().is_empty());
    assert_eq!(messages::delete(&db, &[]).unwrap(), 0);
    messages::status(&db, "one", ChatStatus::Partial, Some(r#"{"results":[]}"#)).unwrap();
    assert_eq!(
        messages::get(&db, "one").unwrap().unwrap().status,
        ChatStatus::Partial
    );
    assert!(messages::content(&db, "one", "malformed").is_err());
    assert_eq!(
        messages::get(&db, "one").unwrap().unwrap().content,
        one.content
    );
    let mut bad = two.clone();
    bad.id = "bad".into();
    bad.content = "bad".into();
    assert!(
        messages::save(
            &db,
            &[
                DChat {
                    id: "not-inserted".into(),
                    ..one
                },
                bad
            ],
            SaveMode::Insert
        )
        .is_err()
    );
    assert!(messages::get(&db, "not-inserted").unwrap().is_none());
    db.with_conn(|c| c.execute_batch("DROP TABLE peers;"))
        .unwrap();
    assert!(peers::all(&db).is_err());
}
#[test]
fn nearby_cache_keeps_newest_facts_and_all_records_survive_reopening() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("db");
    let db = Db::open(&path).unwrap();
    let mut item = DNearbyDeviceCache {
        id: "nearby".into(),
        name: "new".into(),
        ips: vec!["::1".into(), "127.0.0.1".into()],
        port: 443,
        device_type: "PHONE".into(),
        version: "1".into(),
        platform: "Android".into(),
        last_seen: "2026-10-04T00:00:02Z".into(),
    };
    nearby::save(&db, &item).unwrap();
    item.name = "stale".into();
    item.last_seen = "2026-10-04T00:00:01Z".into();
    nearby::save(&db, &item).unwrap();
    drop(db);
    let db = Db::open(&path).unwrap();
    let rows = nearby::all(&db).unwrap();
    assert_eq!(rows[0].name, "new");
    assert_eq!(rows[0].ips.len(), 2);
    assert!(!nearby::touch(&db, "absent").unwrap());
    assert!(nearby::touch(&db, "nearby").unwrap());
    assert!(nearby::delete(&db, "nearby").unwrap());
    assert!(nearby::all(&db).unwrap().is_empty());
}
#[tokio::test]
async fn private_http_chat_store_authenticates_and_reports_database_errors() {
    let temp = tempfile::tempdir().unwrap();
    let token = crate::base64_encode(&[26; 32]);
    let server = ContentServer::start(
        &temp.path().join("db"),
        &token,
        Arc::new(crate::prefs::Prefs::load(&temp.path().join("prefs")).unwrap()),
    )
    .unwrap();
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/chat/store", server.port);
    assert_eq!(
        client
            .post(&url)
            .header("content-type", "application/json")
            .body(json!({"action":"channels"}).to_string())
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let peer = DPeer::new("http-peer", "http", "127.0.0.1", 443, DeviceType::Phone);
    let response = client
        .post(&url)
        .bearer_auth(&token)
        .header("content-type", "application/json")
        .body(json!({"action":"savePeers","mode":"INSERT","items":[peer]}).to_string())
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    let response = client
        .post(&url)
        .bearer_auth(&token)
        .header("content-type", "application/json")
        .body(json!({"action":"peer","id":"http-peer"}).to_string())
        .send()
        .await
        .unwrap();
    let body: Value = serde_json::from_str(&response.text().await.unwrap()).unwrap();
    assert_eq!(body["result"]["name"], "http");
    let db = Db::open(&temp.path().join("db")).unwrap();
    db.with_conn(|c| c.execute_batch("DROP TABLE chat_channels;"))
        .unwrap();
    let response = client
        .post(&url)
        .bearer_auth(&token)
        .header("content-type", "application/json")
        .body(json!({"action":"channels"}).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
}
