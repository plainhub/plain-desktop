//! Behavior locks for `src/library/tags.rs` — ported from plain-nas's
//! `tests/unit/db/tags.rs` minus the KV-era migration tests (SQLite
//! starts clean; there is nothing to migrate).

use super::*;
#[path = "fixtures.rs"]
mod fixtures;
use fixtures::*;

#[test]
fn round_trip_tag() {
    let db = test_db("round_trip");
    let t = create_tag(&db, 3, "vacation").unwrap();
    let got = tag_by_id(&db, &t.id).unwrap();
    assert_eq!(got.name, "vacation");
    assert_eq!(got.kind, 3);
    assert_eq!(got.count, 0);
}

#[test]
fn relation_count_auto_updates() {
    let db = test_db("count_updates");
    let t = create_tag(&db, 1, "music").unwrap();
    add_relations(
        &db,
        &[
            (t.id.clone(), "k1".to_string()),
            (t.id.clone(), "k2".to_string()),
            (t.id.clone(), "k3".to_string()),
        ],
    );
    assert_eq!(tag_by_id(&db, &t.id).unwrap().count, 3);
    remove_relations(
        &db,
        &["k1".to_string(), "k2".to_string()],
        std::slice::from_ref(&t.id),
    );
    assert_eq!(tag_by_id(&db, &t.id).unwrap().count, 1);
}

#[test]
fn add_relations_skips_duplicates_and_empties() {
    let db = test_db("dedup");
    let t = create_tag(&db, 1, "music").unwrap();
    add_relations(&db, &[(t.id.clone(), "k1".to_string())]);
    // Same pair twice + empty ids: one relation total.
    add_relations(
        &db,
        &[
            (t.id.clone(), "k1".to_string()),
            (t.id.clone(), "k2".to_string()),
            ("".to_string(), "k3".to_string()),
            (t.id.clone(), "".to_string()),
        ],
    );
    assert_eq!(tag_by_id(&db, &t.id).unwrap().count, 2);
}

#[test]
fn delete_tag_cascades_relations() {
    let db = test_db("delete_cascade");
    let t1 = create_tag(&db, 1, "a").unwrap();
    let t2 = create_tag(&db, 1, "b").unwrap();
    add_relations(
        &db,
        &[
            (t1.id.clone(), "k1".to_string()),
            (t2.id.clone(), "k1".to_string()),
        ],
    );
    delete_tag(&db, &t1.id);
    assert!(tag_by_id(&db, &t1.id).is_none());
    assert!(
        relations_for_key(&db, "k1")
            .iter()
            .all(|r| r.tag_id == t2.id)
    );
    assert_eq!(tag_by_id(&db, &t2.id).unwrap().count, 1);
}

#[test]
fn update_missing_tag_returns_none() {
    let db = test_db("update_missing");
    assert!(update_tag(&db, "nope", "x").unwrap().is_none());
}

#[test]
fn relations_for_keys_filters_kind_and_keeps_key_order() {
    let db = test_db("kind_filter");
    let audio_tag = create_tag(&db, 1, "audio-tag").unwrap();
    let image_tag = create_tag(&db, 3, "image-tag").unwrap();
    add_relations(
        &db,
        &[
            (audio_tag.id.clone(), "k1".to_string()),
            (image_tag.id.clone(), "k1".to_string()),
        ],
    );
    let rels = relations_for_keys_of_kind(&db, &["k1".to_string()], 3);
    assert_eq!(rels.len(), 1);
    assert_eq!(rels[0].tag_id, image_tag.id);

    // Multi-key: input key order is preserved.
    add_relations(&db, &[(audio_tag.id.clone(), "k2".to_string())]);
    let rels = relations_for_keys_of_kind(&db, &["k2".to_string(), "k1".to_string()], 1);
    assert_eq!(
        rels.iter().map(|r| r.key.as_str()).collect::<Vec<_>>(),
        ["k2", "k1"]
    );
}

#[test]
fn tags_for_key_of_kind_returns_live_counts() {
    let db = test_db("tags_for_key");
    let t = create_tag(&db, 2, "video-tag").unwrap();
    add_relations(
        &db,
        &[
            (t.id.clone(), "m1".to_string()),
            (t.id.clone(), "m2".to_string()),
        ],
    );
    let tags = tags_for_key_of_kind(&db, "m1", 2);
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].count, 2);
    assert!(tags_for_key_of_kind(&db, "m1", 1).is_empty());
}

#[test]
fn remove_relations_for_keys_cascades_media_delete() {
    let db = test_db("media_delete");
    let t = create_tag(&db, 1, "music").unwrap();
    add_relations(
        &db,
        &[
            (t.id.clone(), "gone".to_string()),
            (t.id.clone(), "stays".to_string()),
        ],
    );
    remove_relations_for_keys(&db, &["gone".to_string()]);
    assert!(relations_for_key(&db, "gone").is_empty());
    assert_eq!(relations_for_key(&db, "stays").len(), 1);
    assert_eq!(tag_by_id(&db, &t.id).unwrap().count, 1);
}

#[test]
fn tags_by_type_lists_insertion_order_with_counts() {
    let db = test_db("list_order");
    let a = create_tag(&db, 1, "a").unwrap();
    let b = create_tag(&db, 1, "b").unwrap();
    let _other_kind = create_tag(&db, 3, "c").unwrap();
    add_relations(&db, &[(b.id.clone(), "k".to_string())]);
    let listed = tags_by_type(&db, 1);
    assert_eq!(
        listed.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
        [a.id.as_str(), b.id.as_str()]
    );
    assert_eq!(listed[1].count, 1);
}
