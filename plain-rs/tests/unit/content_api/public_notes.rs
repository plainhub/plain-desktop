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

/// Notes are Rust SQLite rows, so reaching the platform at all is a bug.
fn fixture() -> (tempfile::TempDir, PublicSchema) {
    fixture_with(|method, _| panic!("unexpected host call {method}"))
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

fn database(schema: &PublicSchema) -> &Arc<Db> {
    schema.data::<Arc<Db>>().unwrap()
}

async fn query(schema: &PublicSchema, document: &str) -> Value {
    serde_json::to_value(schema.execute(document).await).unwrap()
}

async fn rows(schema: &PublicSchema, selection: &str) -> Vec<Value> {
    let data = query(schema, &format!("query {{ {selection} }}")).await;
    assert!(data["errors"].as_array().is_none_or(Vec::is_empty), "{data}");
    let (name, _) = selection.split_once('(').expect("a selection with arguments");
    data["data"][name.trim()]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

#[tokio::test]
async fn a_created_note_comes_back_with_the_fields_the_contract_promises() {
    let (_dir, schema) = fixture();
    let created = query(
        &schema,
        r#"mutation { createNote(input: {title: "Trip", content: "Packing"}) {
            id title content deletedAt createdAt tags { name }
        } }"#,
    )
    .await;
    let note = &created["data"]["createNote"];
    assert_eq!(note["title"], "Trip");
    assert_eq!(note["content"], "Packing");
    // Absent rather than the epoch: a live note is not in the trash.
    assert!(note["deletedAt"].is_null(), "{note}");
    assert!(!note["id"].as_str().unwrap_or_default().is_empty());
    assert_eq!(note["tags"].as_array().map(Vec::len), Some(0));
}

#[tokio::test]
async fn an_unknown_note_id_is_null_rather_than_an_error() {
    let (_dir, schema) = fixture();
    let data = query(&schema, r#"query { note(id: "nope") { title } }"#).await;
    assert!(data["errors"].as_array().is_none_or(Vec::is_empty), "{data}");
    assert!(data["data"]["note"].is_null(), "{data}");
}

#[tokio::test]
async fn the_page_and_the_count_agree_on_the_search_dsl() {
    let (_dir, schema) = fixture();
    // A note's `text:` filter searches its content, not its title.
    for (title, content) in [
        ("Groceries", "milk and eggs"),
        ("Hardware", "new screwdriver"),
        ("Holiday plans", "book the hotel"),
    ] {
        let _ = query(
            &schema,
            &format!(
                r#"mutation {{ createNote(input: {{title: "{title}", content: "{content}"}}) {{ id }} }}"#
            ),
        )
        .await;
    }
    let filtered = rows(&schema, r#"notes(query: "text:hotel", offset: 0, limit: 10) { title }"#).await;
    assert_eq!(filtered.len(), 1, "{filtered:?}");
    assert_eq!(filtered[0]["title"], "Holiday plans");

    let count = query(&schema, r#"query { noteCount(query: "text:hotel") }"#).await;
    assert_eq!(count["data"]["noteCount"], 1);
    let all = query(&schema, r#"query { noteCount(query: "") }"#).await;
    assert_eq!(all["data"]["noteCount"], 3);
}

/// The contract's `Tag` has no numeric kind, and the note roots are the one
/// place the app's own note roots could not be reused. If this ever grows a
/// `type` field again the two types are back in the same registry.
#[tokio::test]
async fn a_tagged_note_reports_the_contract_tag_shape() {
    let (_dir, schema) = fixture();
    let library = database(&schema);
    let tag = crate::library::tags::create_tag(library, DataType::Note.kind(), "urgent").unwrap();
    let created = query(
        &schema,
        r#"mutation { createNote(input: {title: "Taxes", content: ""}) { id } }"#,
    )
    .await;
    let id = created["data"]["createNote"]["id"].as_str().unwrap().to_string();
    crate::library::tags::add_relations(
        library,
        &[(tag.id.clone(), id.clone())],
    )
    .unwrap();

    let listed = rows(
        &schema,
        r#"notes(query: "", offset: 0, limit: 10) { title tags { id name count } }"#,
    )
    .await;
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0]["tags"][0]["name"], "urgent");
    assert!(listed[0]["tags"][0].get("type").is_none(), "{listed:?}");
}

/// Deleting reaches the trash, not the live list: the note has to be
/// trashed first, and a delete aimed at a live note counts zero rather than
/// silently taking it.
#[tokio::test]
async fn delete_only_reaches_the_trash_and_reports_what_it_removed() {
    let (_dir, schema) = fixture();
    let created = query(
        &schema,
        r#"mutation { createNote(input: {title: "Temp", content: ""}) { id } }"#,
    )
    .await;
    let id = created["data"]["createNote"]["id"].as_str().unwrap().to_string();

    let early = query(
        &schema,
        &format!(r#"mutation {{ deleteNotes(query: "ids:{id}") {{ affectedCount }} }}"#),
    )
    .await;
    assert_eq!(early["data"]["deleteNotes"]["affectedCount"], 0);
    let still_there = query(&schema, r#"query { noteCount(query: "") }"#).await;
    assert_eq!(still_there["data"]["noteCount"], 1);

    let _ = query(
        &schema,
        &format!(r#"mutation {{ trashNotes(query: "ids:{id}") {{ affectedCount }} }}"#),
    )
    .await;
    let deleted = query(
        &schema,
        &format!(r#"mutation {{ deleteNotes(query: "ids:{id}") {{ affectedCount }} }}"#),
    )
    .await;
    assert_eq!(deleted["data"]["deleteNotes"]["affectedCount"], 1);

    let remaining = query(&schema, r#"query { noteCount(query: "") }"#).await;
    assert_eq!(remaining["data"]["noteCount"], 0);
}

/// A blank query would match every note. The contract asks for `all:true` to
/// mean that on purpose, so the empty string is refused rather than honoured.
#[tokio::test]
async fn a_blank_bulk_query_is_refused_before_the_store_is_touched() {
    let (_dir, schema) = fixture();
    let _ = query(
        &schema,
        r#"mutation { createNote(input: {title: "Keep me", content: ""}) { id } }"#,
    )
    .await;
    let refused = query(&schema, r#"mutation { deleteNotes(query: "") { affectedCount } }"#).await;
    assert!(!refused["errors"].as_array().is_none_or(Vec::is_empty), "{refused}");

    let survivors = query(&schema, r#"query { noteCount(query: "") }"#).await;
    assert_eq!(survivors["data"]["noteCount"], 1);
}

#[tokio::test]
async fn trash_and_restore_move_a_note_between_the_two_lists() {
    let (_dir, schema) = fixture();
    let created = query(
        &schema,
        r#"mutation { createNote(input: {title: "Later", content: ""}) { id } }"#,
    )
    .await;
    let id = created["data"]["createNote"]["id"].as_str().unwrap().to_string();

    let _ = query(
        &schema,
        &format!(r#"mutation {{ trashNotes(query: "ids:{id}") {{ affectedCount }} }}"#),
    )
    .await;
    let after_trash = query(&schema, r#"query { notes(query: "ids:", offset: 0, limit: 10) { title } }"#).await;
    assert!(after_trash["data"]["notes"].as_array().unwrap().is_empty(), "{after_trash}");

    let _ = query(
        &schema,
        &format!(r#"mutation {{ restoreNotes(query: "ids:{id}") {{ affectedCount }} }}"#),
    )
    .await;
    let after_restore = query(&schema, r#"query { notes(query: "", offset: 0, limit: 10) { title } }"#).await;
    assert_eq!(
        after_restore["data"]["notes"]
            .as_array()
            .map(Vec::len),
        Some(1),
        "{after_restore}"
    );
}

#[tokio::test]
async fn updating_a_note_replaces_both_fields() {
    let (_dir, schema) = fixture();
    let created = query(
        &schema,
        r#"mutation { createNote(input: {title: "Draft", content: "v1"}) { id } }"#,
    )
    .await;
    let id = created["data"]["createNote"]["id"].as_str().unwrap().to_string();
    let updated = query(
        &schema,
        &format!(
            r#"mutation {{ updateNote(id: "{id}", input: {{title: "Final", content: "v2"}}) {{
                title content
            }} }}"#
        ),
    )
    .await;
    assert_eq!(updated["data"]["updateNote"]["title"], "Final");
    assert_eq!(updated["data"]["updateNote"]["content"], "v2");
}