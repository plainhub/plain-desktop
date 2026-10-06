use super::*;
use crate::content_api::host::Host;
use crate::content_api::public_schema::PublicSchema;
use crate::db::notes_feeds::FeedEntryRow;
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

const NOW: &str = "2026-01-02T03:04:05+00:00";

fn seed_feed(db: &Arc<Db>, id: &str, name: &str) {
    db.feed_save(
        id,
        name,
        &format!("https://example.com/{id}.xml"),
        false,
        NOW,
    )
    .unwrap();
}

fn seed_entry(db: &Arc<Db>, feed: &str, title: &str, image: &str) {
    db.feed_entries_insert(&[FeedEntryRow {
        id: format!("entry-{title}"),
        feed_id: feed.to_string(),
        title: title.to_string(),
        url: format!("https://example.com/{title}"),
        image: image.to_string(),
        description: String::new(),
        author: String::new(),
        content: String::new(),
        raw_id: format!("raw-{title}"),
        published_at: NOW.to_string(),
        read: false,
        created_at: NOW.to_string(),
        updated_at: NOW.to_string(),
    }])
    .unwrap();
}

#[tokio::test]
async fn an_unknown_feed_is_null_rather_than_an_error() {
    let (_dir, schema) = fixture();
    let data = query(&schema, r#"query { feed(id: "nope") { name } }"#).await;
    assert!(data["errors"].as_array().is_none_or(Vec::is_empty), "{data}");
    assert!(data["data"]["feed"].is_null(), "{data}");
}

#[tokio::test]
async fn feeds_are_listed_with_the_fields_the_contract_promises() {
    let (_dir, schema) = fixture();
    let db = database(&schema);
    seed_feed(db, "one", "Daily news");
    let listed = query(
        &schema,
        r#"query { feeds { id name url fetchContent logo lastSyncAt lastError { code detail } createdAt } }"#,
    )
    .await;
    let rows = listed["data"]["feeds"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{listed}");
    assert_eq!(rows[0]["name"], "Daily news");
    assert_eq!(rows[0]["url"], "https://example.com/one.xml");
    assert_eq!(rows[0]["fetchContent"], false);
    assert_eq!(rows[0]["lastError"]["code"], "");
    // `feed_save` never stamps a sync time, so this is the null branch and
    // not a default zero wearing an Instant.
    assert!(rows[0]["lastSyncAt"].is_null(), "{listed}");
}

/// A client's own url token is what turns a stored reference into a URL, so
/// the contract carries the reference itself. The app's feed roots encrypt
/// the value before returning it; if this ever regresses, a non-http
/// reference comes back as an opaque token the client cannot decrypt.
#[tokio::test]
async fn an_entry_image_is_the_stored_reference_not_a_prebaked_token() {
    let (_dir, schema) = fixture();
    let db = database(&schema);
    seed_feed(db, "images", "Images");
    seed_entry(db, "images", "local", "entry-image.png");
    seed_entry(db, "images", "remote", "https://cdn.example.com/a.png");

    let entries = query(
        &schema,
        r#"query { feedEntries(query: "", offset: 0, limit: 10) { title image } }"#,
    )
    .await;
    let rows = entries["data"]["feedEntries"].as_array().unwrap();
    let by_title = |name: &str| {
        rows.iter()
            .find(|row| row["title"] == name)
            .map(|row| row["image"].clone())
            .unwrap()
    };
    assert_eq!(by_title("local"), "entry-image.png", "{entries}");
    assert_eq!(
        by_title("remote"),
        "https://cdn.example.com/a.png",
        "{entries}"
    );
}

#[tokio::test]
async fn entries_carry_their_feed_and_the_contract_entry_counts() {
    let (_dir, schema) = fixture();
    let db = database(&schema);
    seed_feed(db, "counts", "Counts");
    seed_entry(db, "counts", "first", "");
    seed_entry(db, "counts", "second", "");
    seed_entry(db, "counts", "third", "");

    let entries = query(
        &schema,
        r#"query { feedEntries(query: "", offset: 0, limit: 10) {
            id title rawId tags { name } feed { id }
        } }"#,
    )
    .await;
    let list = entries["data"]["feedEntries"].as_array().unwrap();
    assert_eq!(list.len(), 3, "{entries}");
    // Newest first, and every seeded row shares a timestamp, so the order
    // inside the page is not something to assert on. Look the row up.
    let first = list
        .iter()
        .find(|row| row["title"] == "first")
        .unwrap_or_else(|| panic!("missing the seeded entry: {entries}"));
    assert_eq!(first["rawId"], "raw-first");
    assert_eq!(first["feed"]["id"], "counts");
    assert_eq!(first["tags"].as_array().map(Vec::len), Some(0));

    let counts = query(&schema, r#"query { feedEntryCounts { id count } }"#).await;
    let rows = counts["data"]["feedEntryCounts"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], "counts");
    assert_eq!(rows[0]["count"], 3);

    let total = query(&schema, r#"query { feedEntryCount(query: "") }"#).await;
    assert_eq!(total["data"]["feedEntryCount"], 3);
}

#[tokio::test]
async fn paging_and_the_search_dsl_narrow_the_entry_list() {
    let (_dir, schema) = fixture();
    let db = database(&schema);
    seed_feed(db, "paging", "Paging");
    seed_entry(db, "paging", "alpha", "");
    seed_entry(db, "paging", "beta", "");
    seed_entry(db, "paging", "gamma", "");

    let page = query(
        &schema,
        r#"query { feedEntries(query: "", offset: 0, limit: 2) { title } }"#,
    )
    .await;
    assert_eq!(page["data"]["feedEntries"].as_array().map(Vec::len), Some(2));

    let filtered = query(
        &schema,
        r#"query { feedEntries(query: "text:beta", offset: 0, limit: 10) { title } }"#,
    )
    .await;
    let rows = filtered["data"]["feedEntries"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{filtered}");
    assert_eq!(rows[0]["title"], "beta");
}

/// Same reason as `deleteNotes`: a whole-library delete has to be spelled out.
#[tokio::test]
async fn a_blank_bulk_query_is_refused_before_the_store_is_touched() {
    let (_dir, schema) = fixture();
    let db = database(&schema);
    seed_feed(db, "bulk", "Bulk");
    seed_entry(db, "bulk", "keep", "");

    let refused = query(
        &schema,
        r#"mutation { deleteFeedEntries(query: "") { affectedCount } }"#,
    )
    .await;
    assert!(!refused["errors"].as_array().is_none_or(Vec::is_empty), "{refused}");

    let total = query(&schema, r#"query { feedEntryCount(query: "") }"#).await;
    assert_eq!(total["data"]["feedEntryCount"], 1);
}

#[tokio::test]
async fn marking_read_counts_what_it_actually_flipped() {
    let (_dir, schema) = fixture();
    let db = database(&schema);
    seed_feed(db, "read", "Read");
    seed_entry(db, "read", "one", "");
    seed_entry(db, "read", "two", "");

    // A blank query is refused here too, so the test targets the feed.
    let refused = query(
        &schema,
        r#"mutation { markFeedEntriesRead(query: "", read: true) { affectedCount } }"#,
    )
    .await;
    assert!(!refused["errors"].as_array().is_none_or(Vec::is_empty), "{refused}");

    let marked = query(
        &schema,
        r#"mutation { markFeedEntriesRead(query: "feed_id:read", read: true) { affectedCount } }"#,
    )
    .await;
    assert_eq!(marked["data"]["markFeedEntriesRead"]["affectedCount"], 2);

    let entries = query(
        &schema,
        r#"query { feedEntries(query: "", offset: 0, limit: 10) { read } }"#,
    )
    .await;
    for row in entries["data"]["feedEntries"].as_array().unwrap() {
        assert_eq!(row["read"], true, "{entries}");
    }
}

#[tokio::test]
async fn renaming_a_feed_reports_the_row_it_changed() {
    let (_dir, schema) = fixture();
    seed_feed(database(&schema), "rename", "Old name");
    let updated = query(
        &schema,
        r#"mutation { updateFeed(id: "rename", name: "Daily", fetchContent: true) { name fetchContent } }"#,
    )
    .await;
    assert_eq!(updated["data"]["updateFeed"]["name"], "Daily");
    assert_eq!(updated["data"]["updateFeed"]["fetchContent"], true);
}

#[tokio::test]
async fn changing_a_feed_url_reports_the_new_url() {
    let (_dir, schema) = fixture();
    seed_feed(database(&schema), "moved", "Moved");
    let updated = query(
        &schema,
        r#"mutation { updateFeedUrl(id: "moved", url: "https://other.example/feed.xml") { url } }"#,
    )
    .await;
    assert_eq!(
        updated["data"]["updateFeedUrl"]["url"],
        "https://other.example/feed.xml"
    );
}

#[tokio::test]
async fn deleting_a_feed_takes_its_entries_with_it() {
    let (_dir, schema) = fixture();
    let db = database(&schema);
    seed_feed(db, "gone", "Gone");
    seed_entry(db, "gone", "orphan", "");

    let deleted = query(&schema, r#"mutation { deleteFeed(id: "gone") }"#).await;
    assert_eq!(deleted["data"]["deleteFeed"], true);

    let total = query(&schema, r#"query { feedEntryCount(query: "") }"#).await;
    assert_eq!(total["data"]["feedEntryCount"], 0, "{total}");
}

#[tokio::test]
async fn opml_round_trips_through_export_and_import() {
    let (_dir, schema) = fixture();
    let db = database(&schema);
    seed_feed(db, "round", "Round trip");

    let exported = query(&schema, r#"mutation { exportFeeds }"#).await;
    let opml = exported["data"]["exportFeeds"].as_str().unwrap().to_string();
    assert!(opml.contains("https://example.com/round.xml"), "{opml}");

    query(&schema, r#"mutation { deleteFeed(id: "round") }"#).await;
    let imported = query(
        &schema,
        &format!(
            "mutation {{ importFeeds(content: {}) }}",
            serde_json::to_string(&opml).unwrap()
        ),
    )
    .await;
    assert!(imported["errors"].as_array().is_none_or(Vec::is_empty), "{imported}");

    let feeds = query(&schema, r#"query { feeds { url } }"#).await;
    let urls: Vec<&str> = feeds["data"]["feeds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["url"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(urls, vec!["https://example.com/round.xml"], "{feeds}");
}