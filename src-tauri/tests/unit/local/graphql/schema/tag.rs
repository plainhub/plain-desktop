//! Resolver-level tests for the tags schema module — the shared-core
//! behavior locks live in plain-rs; these cover the GraphQL-shaped
//! mapping (kind ordinal, ids: parsing, per-item relation edits).
use super::*;

fn test_library() -> plain_rs::library::db::LibraryDb {
    let dir = std::env::temp_dir().join(format!("plain-desktop-tagrs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    plain_rs::library::db::LibraryDb::open(&dir.join("library.db")).unwrap()
}

#[test]
fn kind_of_maps_every_data_type_ordinal() {
    use crate::local::enums::DataType;
    assert_eq!(kind_of(DataType::Default), 0);
    assert_eq!(kind_of(DataType::Audio), 1);
    assert_eq!(kind_of(DataType::Video), 2);
    assert_eq!(kind_of(DataType::Image), 3);
    assert_eq!(kind_of(DataType::Sms), 4);
    assert_eq!(kind_of(DataType::Contact), 5);
    assert_eq!(kind_of(DataType::Note), 6);
    assert_eq!(kind_of(DataType::FeedEntry), 7);
    assert_eq!(kind_of(DataType::Call), 8);
    assert_eq!(kind_of(DataType::Package), 9);
    assert_eq!(kind_of(DataType::File), 10);
    assert_eq!(kind_of(DataType::AppFile), 11);
    assert_eq!(kind_of(DataType::Doc), 12);
}

#[test]
fn parse_ids_query_takes_explicit_ids_only() {
    // The DSL tokenizer splits on whitespace — real selections are
    // comma-joined without spaces (same on NAS/phone).
    assert_eq!(
        parse_ids_query("ids:a,b,c"),
        Some(vec!["a".to_string(), "b".to_string(), "c".to_string()])
    );
    assert_eq!(
        parse_ids_query("ids:"),
        None,
        "empty ids is not a selection"
    );
    assert_eq!(
        parse_ids_query("text:foo trash:false"),
        None,
        "page DSL is unresolvable without a media index"
    );
}

#[test]
fn core_round_trip_through_the_store_the_resolvers_use() {
    let db = test_library();
    let t = tag_store::create_tag(&db, 1, "cardio").unwrap();
    tag_store::add_relations(
        &db,
        &[
            (t.id.clone(), "k1".to_string()),
            (t.id.clone(), "k2".to_string()),
        ],
    );
    assert_eq!(tag_store::tags_by_type(&db, 1)[0].count, 2);
    let rels = tag_store::relations_for_keys_of_kind(&db, &["k1".to_string()], 1);
    assert_eq!(rels.len(), 1);
    assert_eq!(rels[0].tag_id, t.id);
    // Cross-kind filtering keeps relations out of unrelated type lists.
    assert!(tag_store::relations_for_keys_of_kind(&db, &["k1".to_string()], 3).is_empty());
}
