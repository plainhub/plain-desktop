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
    let ids: std::collections::HashSet<&str> = rows
        .iter()
        .map(|row| row["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains("m2"), "{latest}");
    assert!(ids.contains("c1"), "{latest}");
    assert!(!ids.contains("m1"), "{latest}");
}

// ── the mutations, which are commands rather than store writes ──────────

fn chat_fixture() -> (tempfile::TempDir, PublicSchema) {
    fixture_with(|method, _| match method {
        "systemChatSend" => json!([{
            "id": "new-1", "fromId": "me", "toId": "peer-1", "channelId": "",
            "content": "{\"type\":\"TEXT\"}", "status": "PENDING",
            "statusData": "", "createdAt": T2, "updatedAt": T2,
        }]),
        "systemChatDeleteOne" => json!(true),
        "systemChatDeleteQuery" => json!(3),
        "systemChatRetry" => json!({
            "id": "new-1", "fromId": "me", "toId": "peer-1", "channelId": "",
            "content": "{\"type\":\"TEXT\"}", "status": "PENDING",
            "statusData": "", "createdAt": T2, "updatedAt": T2,
        }),
        _other => panic!("unexpected host call {_other}"),
    })
}

#[tokio::test]
async fn sending_reports_the_queued_row_not_the_delivered_one() {
    let (_dir, schema) = chat_fixture();
    let sent = query(
        &schema,
        r#"mutation { sendChatItem(target: "peer-1", content: "{\"type\":\"TEXT\"}") {
            id fromId toId channelId status
        } }"#,
    )
    .await;
    let rows = sent["data"]["sendChatItem"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{sent}");
    assert_eq!(rows[0]["status"], "PENDING", "{sent}");
    assert!(rows[0]["channelId"].is_null(), "{sent}");
}

#[tokio::test]
async fn bulk_delete_counts_what_it_removed() {
    let (_dir, schema) = chat_fixture();
    let deleted = query(
        &schema,
        r#"mutation { deleteChatItems(query: "channel:chan-1") { affectedCount } }"#,
    )
    .await;
    assert_eq!(deleted["data"]["deleteChatItems"]["affectedCount"], 3, "{deleted}");
}

#[tokio::test]
async fn deleting_one_item_is_always_true() {
    let (_dir, schema) = chat_fixture();
    let deleted = query(&schema, r#"mutation { deleteChatItem(id: "m1") }"#).await;
    assert_eq!(deleted["data"]["deleteChatItem"], true, "{deleted}");
}

#[tokio::test]
async fn retrying_reports_the_row_it_requeued() {
    let (_dir, schema) = chat_fixture();
    let retried = query(
        &schema,
        r#"mutation { retryChatItem(id: "new-1") { id status } }"#,
    )
    .await;
    assert_eq!(retried["data"]["retryChatItem"]["status"], "PENDING", "{retried}");
}

/// A retry for an id that is not in the store has no row to report, and the
/// contract types it non-null — so this is the one case that errors.
#[tokio::test]
async fn retrying_an_unknown_id_is_an_error() {
    let (_dir, schema) = fixture_with(|_method, _| Value::Null);
    let retried = query(&schema, r#"mutation { retryChatItem(id: "nope") { id } }"#).await;
    assert!(!retried["errors"].as_array().is_none_or(Vec::is_empty), "{retried}");
}

// ── channels ────────────────────────────────────────────────────────────

fn channel_fixture() -> (tempfile::TempDir, PublicSchema) {
    fixture_with(|method, params| match method {
        "systemChatChannelFacts" => json!([channel("chan-1", "General", 3, "JOINED")]),
        "systemChatChannelAction" => match params["action"].as_str().unwrap_or_default() {
            "create" => channel("chan-2", "New", 1, "JOINED"),
            "rename" => channel("chan-2", "Renamed", 1, "JOINED"),
            "invite" | "kick" => channel("chan-1", "General", 3, "JOINED"),
            _ => json!(true),
        },
        _other => panic!("unexpected host call {_other}"),
    })
}

fn channel(id: &str, name: &str, version: i64, status: &str) -> Value {
    json!({
        "id": id, "ownerId": "me", "name": name,
        "members": [
            { "peerId": "me", "status": "JOINED" },
            { "peerId": "peer-9", "status": "PENDING" },
        ],
        "version": version, "status": status,
        "createdAt": T1, "updatedAt": T2,
    })
}

#[tokio::test]
async fn channels_report_their_members_and_version() {
    let (_dir, schema) = channel_fixture();
    let listed = query(
        &schema,
        r#"query { chatChannels { id ownerId name members { peerId status } version status } }"#,
    )
    .await;
    let rows = listed["data"]["chatChannels"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{listed}");
    assert_eq!(rows[0]["version"], 3);
    assert_eq!(rows[0]["status"], "JOINED");
    let members = rows[0]["members"].as_array().unwrap();
    assert_eq!(members[0]["peerId"], "me");
    assert_eq!(members[1]["status"], "PENDING");
}

#[tokio::test]
async fn creating_and_renaming_a_channel_report_the_new_row() {
    let (_dir, schema) = channel_fixture();
    let created = query(&schema, r#"mutation { createChatChannel(name: "New") { id name } }"#).await;
    assert_eq!(created["data"]["createChatChannel"]["name"], "New", "{created}");

    let renamed = query(
        &schema,
        r#"mutation { updateChatChannel(id: "chan-2", name: "Renamed") { name version } }"#,
    )
    .await;
    assert_eq!(renamed["data"]["updateChatChannel"]["name"], "Renamed", "{renamed}");
    assert_eq!(renamed["data"]["updateChatChannel"]["version"], 1);
}

#[tokio::test]
async fn membership_changes_report_the_channel_they_changed() {
    let (_dir, schema) = channel_fixture();
    let invited = query(
        &schema,
        r#"mutation { addChatChannelMember(id: "chan-1", peerId: "peer-9") { id members { peerId } } }"#,
    )
    .await;
    assert_eq!(
        invited["data"]["addChatChannelMember"]["members"]
            .as_array()
            .map(Vec::len),
        Some(2),
        "{invited}"
    );

    let kicked = query(
        &schema,
        r#"mutation { removeChatChannelMember(id: "chan-1", peerId: "peer-9") { id } }"#,
    )
    .await;
    assert_eq!(kicked["data"]["removeChatChannelMember"]["id"], "chan-1", "{kicked}");
}

#[tokio::test]
async fn the_remaining_channel_mutations_acknowledge_the_request() {
    let (_dir, schema) = channel_fixture();
    for document in [
        r#"mutation { deleteChatChannel(id: "chan-1") }"#,
        r#"mutation { leaveChatChannel(id: "chan-1") }"#,
        r#"mutation { acceptChatChannelInvite(id: "chan-1") }"#,
        r#"mutation { declineChatChannelInvite(id: "chan-1") }"#,
    ] {
        let result = query(&schema, document).await;
        assert!(result["errors"].as_array().is_none_or(Vec::is_empty), "{result}");
        let (field, _) = document
            .trim_start_matches("mutation { ")
            .split_once('(')
            .unwrap();
        assert_eq!(result["data"][field], true, "{result}");
    }
}