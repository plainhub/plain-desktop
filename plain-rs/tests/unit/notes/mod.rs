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
    tags::add_relations(&db, &[(tag.id, a.id.clone())]).unwrap();
    assert_eq!(trash(&db, &format!("ids:{}", a.id)).unwrap(), 1);
    assert_eq!(count(&db, "").unwrap(), 1);
    assert_eq!(count(&db, "trash:true").unwrap(), 1);
    assert!(
        tags::tags_for_key_of_kind(&db, &a.id, DataType::Note.kind())
            .unwrap()
            .is_empty()
    );
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
    )
    .unwrap();
    assert_eq!(trash(&db, "ids:entry").unwrap(), 1);
    assert_eq!(
        tags::tags_for_key_of_kind(&db, "entry", DataType::FeedEntry.kind())
            .unwrap()
            .len(),
        1
    );
    restore(&db, "ids:entry").unwrap();
    tags::add_relations(&db, &[(note_tag.id, "entry".into())]).unwrap();
    assert_eq!(crate::feeds::delete_entries(&db, "ids:entry").unwrap(), 1);
    assert_eq!(
        tags::tags_for_key_of_kind(&db, "entry", DataType::Note.kind())
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn markdown_titles_are_derived_in_rust() {
    assert_eq!(markdown_title("## Sub\nsome text\n# Real\nmore"), "Real");
    assert_eq!(markdown_title("![alt][id]"), "🖼");
    assert_eq!(markdown_title("<IMG src='x.png'>"), "🖼");
    assert_eq!(markdown_title("a\nb"), "ab");
}

/// A bulk query naming a field the table does not understand used to be
/// silently dropped, leaving the clause list empty — which renders as `1=1`.
/// For a destructive mutation that is the whole table, and it looks exactly
/// like the `all:true` sentinel to the caller.
#[test]
fn unknown_and_inert_query_fields_are_rejected_instead_of_widening() {
    let (_dir, db) = database();
    create(&db, "First", "one").unwrap();
    create(&db, "Second", "two").unwrap();

    // `id` is not a field this table knows — `ids` is — so it used to be
    // dropped and `deleteNotes`/`trashNotes` hit every row.
    let err = trash(&db, "id:__probe__").unwrap_err().to_string();
    assert!(err.contains("unsupported note filter: id"), "{err}");
    let err = delete(&db, "zzz_no_such_field:x").unwrap_err().to_string();
    assert!(
        err.contains("unsupported note filter: zzz_no_such_field"),
        "{err}"
    );

    // A field that parses but narrows nothing is the same hazard by another
    // route, so it is refused too rather than widening to every row.
    let err = trash(&db, "text:").unwrap_err().to_string();
    assert!(err.contains("selects no note rows"), "{err}");

    // Nothing was touched along the way.
    assert_eq!(count(&db, "").unwrap(), 2);

    // The supported vocabulary still works, including the explicit sentinel
    // that is allowed to select everything on purpose.
    assert_eq!(count(&db, "text:one").unwrap(), 1);
    assert_eq!(count(&db, "ids:__probe__").unwrap(), 0);
    assert_eq!(trash(&db, "all:true").unwrap(), 2);
}

/// `trash` scopes the notes table; the feed-entry table has no `deleted_at`,
/// so accepting it there would be a field that silently does nothing.
#[test]
fn note_only_fields_are_rejected_on_feed_entries() {
    let (_dir, db) = database();
    let err = db.feed_entries_list("trash:true", 10, 0).unwrap_err().to_string();
    assert!(err.contains("unsupported feed entry filter: trash"), "{err}");
    // The feed-only fields are the ones that belong there.
    assert!(db.feed_entries_list("read:true", 10, 0).is_ok());
}
