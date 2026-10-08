use super::*;
use crate::content_api::host::Host;
use crate::content_api::public_schema::PublicSchema;
use crate::db::Db;
use crate::prefs::Prefs;
use serde_json::{Value, json};
use std::sync::Arc;

fn stub(host: Arc<Host>, handler: impl Fn(&str, Value) -> Value + Send + 'static) {
    let (generation, mut requests) = host.connect();
    let host = host.clone();
    tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            let Some(id) = request["id"].as_u64() else {
                continue;
            };
            let result = handler(
                &request["method"].as_str().unwrap_or_default(),
                request["params"].clone(),
            );
            let _ = host.reply(generation, json!({ "id": id, "result": result }));
        }
    });
}

fn fixture_with(
    handler: impl Fn(&str, Value) -> Value + Send + 'static,
) -> (tempfile::TempDir, PublicSchema) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(Prefs::load(&dir.path().join("prefs.json")).unwrap());
    let db = Arc::new(Db::open(&dir.path().join("data.db")).unwrap());
    let host = Arc::new(Host::default());
    stub(host.clone(), handler);
    let directory = dir.path().to_path_buf();
    let (events, _) = tokio::sync::broadcast::channel(16);
    (
        dir,
        crate::content_api::public_schema::build(host, events, prefs, db, directory),
    )
}

/// The message reads must not touch the platform: they are the same store
/// queries the app's own chat list runs.
fn fixture() -> (tempfile::TempDir, PublicSchema) {
    fixture_with(|method, _| panic!("unexpected host call {method}"))
}

fn database(schema: &PublicSchema) -> &Arc<Db> {
    schema.data::<Arc<Db>>().unwrap()
}

async fn query(schema: &PublicSchema, document: &str) -> Value {
    serde_json::to_value(schema.execute(document).await).unwrap()
}

fn seed(db: &Arc<Db>, id: &str, from: &str, to: &str, channel: &str, content: &str, at: &str) {
    db.insert_chat(&DChat {
        id: id.to_string(),
        from_id: from.to_string(),
        to_id: to.to_string(),
        channel_id: channel.to_string(),
        content: content.to_string(),
        status: crate::chat::enums::ChatStatus::Sent,
        status_data: String::new(),
        created_at: at.to_string(),
        updated_at: at.to_string(),
    });
}

const T1: &str = "2026-01-02T03:04:05+00:00";
const T2: &str = "2026-01-02T04:04:05+00:00";

/// `channelId` is "" in the store for a direct message. The contract says
/// null, and a client that filters on it must not have to treat "" as a
/// channel that happens to be empty.
#[tokio::test]
async fn a_direct_message_has_no_channel_id() {
    let (_dir, schema) = fixture();
    seed(database(&schema), "m1", "me", "peer-1", "", "hello", T1);

    let items = query(
        &schema,
        r#"query { chatItems(target: "peer-1", offset: 0, limit: 10, query: "") {
            id fromId toId channelId content status statusData createdAt updatedAt
        } }"#,
    )
    .await;
    let rows = items["data"]["chatItems"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{items}");
    let row = &rows[0];
    assert!(row["channelId"].is_null(), "{items}");
    assert_eq!(row["fromId"], "me");
    assert_eq!(row["toId"], "peer-1");
    assert_eq!(row["content"], "hello");
    assert_eq!(row["status"], "SENT");
}

/// A `peer:` prefix names the same conversation as the bare id, because the
/// app's own send path writes both.
#[tokio::test]
async fn a_peer_prefixed_target_names_the_same_conversation() {
    let (_dir, schema) = fixture();
    seed(database(&schema), "m1", "me", "peer-1", "", "hello", T1);

    let prefixed = query(
        &schema,
        r#"query { chatItems(target: "peer:peer-1", offset: 0, limit: 10, query: "") { id } }"#,
    )
    .await;
    assert_eq!(
        prefixed["data"]["chatItems"].as_array().map(Vec::len),
        Some(1),
        "{prefixed}"
    );
}

#[tokio::test]
async fn a_channel_message_carries_its_channel_and_stays_out_of_the_peer_list() {
    let (_dir, schema) = fixture();
    let db = database(&schema);
    seed(db, "c1", "me", "", "chan-1", "for the channel", T1);
    seed(db, "m1", "me", "peer-1", "", "direct", T2);

    let channel = query(
        &schema,
        r#"query { chatItems(target: "channel:chan-1", offset: 0, limit: 10, query: "") {
            id channelId toId
        } }"#,
    )
    .await;
    let rows = channel["data"]["chatItems"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{channel}");
    assert_eq!(rows[0]["channelId"], "chan-1");

    let peer = query(
        &schema,
        r#"query { chatItems(target: "peer-1", offset: 0, limit: 10, query: "") { id } }"#,
    )
    .await;
    assert_eq!(
        peer["data"]["chatItems"].as_array().map(Vec::len),
        Some(1),
        "{peer}"
    );
}

/// The page is newest-first internally but handed back oldest-first, which
/// is what lets a client render the page in order without reversing it.
#[tokio::test]
async fn a_page_comes_back_oldest_first() {
    let (_dir, schema) = fixture();
    let db = database(&schema);
    seed(db, "m1", "me", "peer-1", "", "first", T1);
    seed(db, "m2", "me", "peer-1", "", "second", T2);

    let items = query(
        &schema,
        r#"query { chatItems(target: "peer-1", offset: 0, limit: 10, query: "") { id content } }"#,
    )
    .await;
    let rows = items["data"]["chatItems"].as_array().unwrap();
    let ids: Vec<&str> = rows.iter().map(|row| row["id"].as_str().unwrap()).collect();
    assert_eq!(ids, vec!["m1", "m2"], "{items}");
}

#[tokio::test]
async fn the_text_field_filters_before_the_page_is_cut() {
    let (_dir, schema) = fixture();
    let db = database(&schema);
    seed(db, "m1", "me", "peer-1", "", "buy milk", T1);
    seed(db, "m2", "me", "peer-1", "", "call mum", T2);
    seed(db, "m3", "me", "peer-1", "", "buy bread", T2);

    let items = query(
        &schema,
        r#"query { chatItems(target: "peer-1", offset: 0, limit: 10, query: "text:buy") { content } }"#,
    )
    .await;
    let rows = items["data"]["chatItems"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "{items}");
}

#[tokio::test]
async fn the_latest_page_covers_every_conversation() {
    let (_dir, schema) = fixture();
    let db = database(&schema);
    seed(db, "m1", "me", "peer-1", "", "first", T1);
    seed(db, "m2", "me", "peer-1", "", "second", T2);
    seed(db, "c1", "me", "", "chan-1", "channel note", T2);

    let latest = query(&schema, r#"query { latestChatItems { id content } }"#).await;
    let rows = latest["data"]["latestChatItems"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "{latest}");
    let ids: std::collections::HashSet<&str> =
        rows.iter().map(|row| row["id"].as_str().unwrap()).collect();
    assert!(ids.contains("m2"), "{latest}");
    assert!(ids.contains("c1"), "{latest}");
    assert!(!ids.contains("m1"), "{latest}");
}

// ── the mutations, which are commands rather than store writes ──────────

#[cfg(feature = "http_transport")]
fn chat_fixture() -> (tempfile::TempDir, super::super::ContentServer) {
    channel_fixture()
}

#[cfg(feature = "http_transport")]
#[tokio::test]
async fn sending_persists_and_emits_without_native_business_callbacks() {
    let (_dir, server) = chat_fixture();
    let state = server.runtime_state();
    let mut events = state.events.subscribe();
    let sent = channel_query(&state, r#"mutation { sendChatItem(target: "peer:local", content: "{\"type\":\"TEXT\",\"value\":\"hello\"}") { id fromId toId channelId status } }"#).await;
    let rows = sent["data"]["sendChatItem"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{sent}");
    assert!(rows[0]["channelId"].is_null(), "{sent}");
    assert!(
        crate::db::chat_store::messages::get(&state.db, rows[0]["id"].as_str().unwrap())
            .unwrap()
            .is_some()
    );
    assert_eq!(
        events.recv().await.unwrap().event_type,
        crate::chat::events::WS_MESSAGE_CREATED
    );
    server.shutdown().await;
}

#[cfg(feature = "http_transport")]
#[tokio::test]
async fn bulk_delete_counts_persisted_rows() {
    let (_dir, server) = chat_fixture();
    let state = server.runtime_state();
    for id in ["m1", "m2", "m3"] {
        seed(&state.db, id, "me", "", "chan-1", "hello", T1);
    }
    let deleted = channel_query(
        &state,
        r#"mutation { deleteChatItems(query: "channel:chan-1") { affectedCount } }"#,
    )
    .await;
    assert_eq!(
        deleted["data"]["deleteChatItems"]["affectedCount"], 3,
        "{deleted}"
    );
    assert!(
        crate::db::chat_store::messages::get(&state.db, "m1")
            .unwrap()
            .is_none()
    );
    server.shutdown().await;
}

#[cfg(feature = "http_transport")]
#[tokio::test]
async fn deleting_one_item_is_idempotent() {
    let (_dir, server) = chat_fixture();
    let state = server.runtime_state();
    seed(&state.db, "m1", "me", "local", "", "hello", T1);
    for _ in 0..2 {
        let deleted = channel_query(&state, r#"mutation { deleteChatItem(id: "m1") }"#).await;
        assert_eq!(deleted["data"]["deleteChatItem"], true, "{deleted}");
    }
    server.shutdown().await;
}

#[cfg(feature = "http_transport")]
#[tokio::test]
async fn retrying_reports_the_persisted_row() {
    let (_dir, server) = chat_fixture();
    let state = server.runtime_state();
    seed(
        &state.db,
        "new-1",
        "me",
        "local",
        "",
        r#"{"type":"TEXT","value":"hello"}"#,
        T1,
    );
    let retried = channel_query(
        &state,
        r#"mutation { retryChatItem(id: "new-1") { id status } }"#,
    )
    .await;
    assert_eq!(retried["data"]["retryChatItem"]["id"], "new-1", "{retried}");
    server.shutdown().await;
}

#[cfg(feature = "http_transport")]
#[tokio::test]
async fn retrying_an_unknown_id_is_an_error() {
    let (_dir, server) = chat_fixture();
    let state = server.runtime_state();
    let retried = channel_query(&state, r#"mutation { retryChatItem(id: "nope") { id } }"#).await;
    assert!(
        !retried["errors"].as_array().is_none_or(Vec::is_empty),
        "{retried}"
    );
    server.shutdown().await;
}

// Channel roots execute against the same runtime as the mobile endpoint.
#[cfg(feature = "http_transport")]
fn channel_fixture() -> (tempfile::TempDir, super::super::ContentServer) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(Prefs::load(&dir.path().join("prefs.json")).unwrap());
    prefs.set("client_id", "actor").unwrap();
    let server = super::super::ContentServer::start(
        &dir.path().join("plain.db"),
        &crate::base64_encode(&[3; 32]),
        prefs,
    )
    .unwrap();
    (dir, server)
}
#[cfg(feature = "http_transport")]
async fn channel_query(state: &super::super::server::ServerState, document: &str) -> Value {
    let bytes = super::super::main_graphql::run(
        &state.public,
        &json!({"query":document}).to_string(),
        Some(state.clone()),
    )
    .await
    .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}
#[cfg(feature = "http_transport")]
#[tokio::test]
async fn channel_roots_read_current_store_and_share_runtime_mutations_without_host() {
    let (_dir, server) = channel_fixture();
    let state = server.runtime_state();
    let mut events = state.events.subscribe();
    let created = channel_query(&state, r#"mutation { createChatChannel(name:"General") { id name ownerId version members { peerId status } } }"#).await;
    assert!(
        created["errors"].as_array().is_none_or(Vec::is_empty),
        "{created}"
    );
    let id = created["data"]["createChatChannel"]["id"].as_str().unwrap();
    assert_eq!(created["data"]["createChatChannel"]["ownerId"], "actor");
    let event: Value = serde_json::from_str(&events.recv().await.unwrap().payload).unwrap();
    assert_eq!(event["channels"][0]["id"], id);
    assert!(event["channels"][0].get("key").is_none());
    assert_eq!(event["channels"][0].as_object().unwrap().len(), 8);
    assert_eq!(event["channels"][0]["members"][0]["peerId"], "actor");
    let renamed = channel_query(
        &state,
        &format!(
            r#"mutation {{ updateChatChannel(id:"{id}",name:"Renamed") {{ name version }} }}"#
        ),
    )
    .await;
    assert_eq!(
        renamed["data"]["updateChatChannel"]["name"], "Renamed",
        "{renamed}"
    );
    assert_eq!(renamed["data"]["updateChatChannel"]["version"], 2);
    let invited = channel_query(&state, &format!(r#"mutation {{ addChatChannelMember(id:"{id}",peerId:"unknown") {{ members {{ peerId status }} version }} }}"#)).await;
    assert_eq!(
        invited["data"]["addChatChannelMember"]["version"], 3,
        "{invited}"
    );
    assert_eq!(
        invited["data"]["addChatChannelMember"]["members"][1]["status"],
        "PENDING"
    );
    let listed = channel_query(
        &state,
        "{ chatChannels { id name version members { peerId status } } }",
    )
    .await;
    assert_eq!(
        listed["data"]["chatChannels"][0]["name"], "Renamed",
        "{listed}"
    );
    assert_eq!(listed["data"]["chatChannels"][0]["version"], 3);
    let kicked = channel_query(&state, &format!(r#"mutation {{ removeChatChannelMember(id:"{id}",peerId:"unknown") {{ version members {{ peerId }} }} }}"#)).await;
    assert_eq!(
        kicked["data"]["removeChatChannelMember"]["version"], 4,
        "{kicked}"
    );
    assert_eq!(
        kicked["data"]["removeChatChannelMember"]["members"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    for action in ["leaveChatChannel", "acceptChatChannelInvite"] {
        let rejected =
            channel_query(&state, &format!(r#"mutation {{ {action}(id:"{id}") }}"#)).await;
        assert!(
            !rejected["errors"].as_array().is_none_or(Vec::is_empty),
            "{rejected}"
        );
    }
    let deleted = channel_query(
        &state,
        &format!(r#"mutation {{ deleteChatChannel(id:"{id}") }}"#),
    )
    .await;
    assert_eq!(deleted["data"]["deleteChatChannel"], true, "{deleted}");
    assert!(
        crate::db::chat_store::channels::get(&state.db, id)
            .unwrap()
            .is_none()
    );
    let missing = channel_query(
        &state,
        r#"mutation { declineChatChannelInvite(id:"missing") }"#,
    )
    .await;
    assert!(
        !missing["errors"].as_array().is_none_or(Vec::is_empty),
        "{missing}"
    );
    let mut last = Value::Null;
    while let Ok(event) = events.try_recv() {
        last = serde_json::from_str(&event.payload).unwrap();
    }
    assert_eq!(last["channels"], json!([]));
    server.shutdown().await;
}
#[cfg(feature = "http_transport")]
#[tokio::test]
async fn channel_query_reads_all_statuses_sorted_by_name_without_native_cache() {
    let (_dir, server) = channel_fixture();
    let state = server.runtime_state();
    for (name, status) in [
        ("Zed", crate::chat::enums::ChannelStatus::Joined),
        ("Alpha", crate::chat::enums::ChannelStatus::Left),
    ] {
        let mut row = crate::chat::channel::state::create(&state.db, "actor", name).unwrap();
        row.status = status;
        crate::db::chat_store::channels::save(
            &state.db,
            &[row],
            crate::db::chat_store::SaveMode::Update,
        )
        .unwrap();
    }
    let result = channel_query(&state, "{ chatChannels { name status } }").await;
    assert_eq!(
        result["data"]["chatChannels"],
        json!([{"name":"Alpha","status":"LEFT"},{"name":"Zed","status":"JOINED"}]),
        "{result}"
    );
    server.shutdown().await;
}

#[cfg(feature = "http_transport")]
#[tokio::test]
async fn channel_snapshot_recovers_current_public_list_without_emitting_another_event() {
    let (_dir, server) = channel_fixture();
    let state = server.runtime_state();
    let channel =
        crate::chat::channel::state::create(&state.db, "actor", "before-reconnect").unwrap();
    let mut events = state.events.subscribe();
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/chat/channel", server.port))
        .bearer_auth(crate::base64_encode(&[3; 32]))
        .header("Content-Type", "application/json")
        .body(json!({"action":"snapshot"}).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let snapshot: Value = serde_json::from_str(&response.text().await.unwrap()).unwrap();
    assert_eq!(snapshot["result"]["channels"][0]["id"], channel.id);
    assert!(snapshot["result"]["channels"][0].get("key").is_none());
    assert!(events.try_recv().is_err());
    server.shutdown().await;
}
