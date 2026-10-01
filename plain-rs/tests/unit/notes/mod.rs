use super::*;
use crate::db::notes_feeds::FeedEntryRow;

fn database() -> (tempfile::TempDir, Db) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("plain.db")).unwrap();
    (dir, db)
}

#[test]
fn note_search_bulk_lifecycle_and_tags() {
    let (_dir, db) = database();
    let a = create(&db, "First", "needle text").unwrap();
    let b = create(&db, "Second", "other text").unwrap();
    assert_eq!(count(&db, "text:needle").unwrap(), 1);
    assert_eq!(
        search(&db, &format!("ids:{}", a.id), 10, 0).unwrap().len(),
        1
    );
    let tag = tags::create_tag(&db, DataType::Note.kind(), "work").unwrap();
    tags::add_relations(&db, &[(tag.id, a.id.clone())]);
    assert_eq!(trash(&db, &format!("ids:{}", a.id)).unwrap(), 1);
    assert_eq!(count(&db, "").unwrap(), 1);
    assert_eq!(count(&db, "trash:true").unwrap(), 1);
    assert!(tags::tags_for_key_of_kind(&db, &a.id, DataType::Note.kind()).is_empty());
    assert_eq!(restore(&db, &format!("ids:{}", a.id)).unwrap(), 1);
    assert_eq!(count(&db, "").unwrap(), 2);
    assert_eq!(trash(&db, "all:true").unwrap(), 2);
    assert_eq!(delete(&db, "all:true").unwrap(), 2);
    assert_eq!(count(&db, "trash:true").unwrap(), 0);
    assert!(update(&db, &b.id, "Changed", "more").is_err());
    assert!(trash(&db, " ").is_err());
}

#[test]
fn export_and_save_feed_entries() {
    let (_dir, db) = database();
    let at = now();
    db.feed_save("feed", "News", "https://example.org/rss", false, &at)
        .unwrap();
    db.feed_entries_insert(&[FeedEntryRow {
        id: "entry".into(),
        feed_id: "feed".into(),
        title: "Headline".into(),
        url: "https://example.org/one".into(),
        image: String::new(),
        description: "Summary".into(),
        author: String::new(),
        content: String::new(),
        raw_id: "hash".into(),
        published_at: at.clone(),
        read: false,
        created_at: at.clone(),
        updated_at: at,
    }])
    .unwrap();
    assert_eq!(
        save_feed_entries(&db, "feed_id:feed").unwrap(),
        vec!["entry"]
    );
    assert_eq!(
        get(&db, "entry").unwrap().unwrap().content,
        "# Headline\n\nSummary"
    );
    let exported: serde_json::Value = serde_json::from_str(&export(&db, "").unwrap()).unwrap();
    assert_eq!(exported[0]["id"], "entry");
    assert_eq!(exported[0]["tags"], serde_json::json!([]));
    let note_tag = tags::create_tag(&db, DataType::Note.kind(), "note").unwrap();
    let feed_tag = tags::create_tag(&db, DataType::FeedEntry.kind(), "feed").unwrap();
    tags::add_relations(
        &db,
        &[
            (note_tag.id.clone(), "entry".into()),
            (feed_tag.id.clone(), "entry".into()),
        ],
    );
    assert_eq!(trash(&db, "ids:entry").unwrap(), 1);
    assert_eq!(
        tags::tags_for_key_of_kind(&db, "entry", DataType::FeedEntry.kind()).len(),
        1
    );
    restore(&db, "ids:entry").unwrap();
    tags::add_relations(&db, &[(note_tag.id, "entry".into())]);
    assert_eq!(crate::feeds::delete_entries(&db, "ids:entry").unwrap(), 1);
    assert_eq!(
        tags::tags_for_key_of_kind(&db, "entry", DataType::Note.kind()).len(),
        1
    );
}
