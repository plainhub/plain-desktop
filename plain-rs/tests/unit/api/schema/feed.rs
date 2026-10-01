use super::*;
use async_graphql::Request;

#[tokio::test]
async fn graphql_feed_entry_joins_and_counts() {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(Db::open(&dir.path().join("plain.db")).unwrap());
    let at = chrono::Utc::now().to_rfc3339();
    db.feed_save("feed", "News", "https://example.org/rss", false, &at)
        .unwrap();
    db.feed_entries_insert(&[FeedEntryRow {
        id: "entry".into(),
        feed_id: "feed".into(),
        title: "Headline".into(),
        url: "https://example.org/article".into(),
        image: String::new(),
        description: "Summary".into(),
        author: String::new(),
        content: String::new(),
        raw_id: "hash".into(),
        published_at: at.clone(),
        created_at: at.clone(),
        updated_at: at,
    }])
    .unwrap();
    let schema = crate::http_server::main_schemas::build_schema();
    let response = schema.execute(Request::new(r#"{ feedEntry(id: "entry") { title tags { id } feed { name } } feedEntries(offset: 0, limit: 10, query: "feed_id:feed") { id feedId } feedEntryCount(query: "feed_id:feed") feedEntryCounts { id count } }"#).data(db)).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    assert_eq!(data["feedEntry"]["feed"]["name"], "News");
    assert_eq!(data["feedEntries"][0]["feedId"], "feed");
    assert_eq!(data["feedEntryCount"], 1);
    assert_eq!(data["feedEntryCounts"][0]["count"], 1);
}
