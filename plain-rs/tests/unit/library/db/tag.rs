//! Unit tests for `src/library/db/tag.rs` (row IO layer).

#[path = "../fixtures.rs"]
mod fixtures;
use fixtures::*;

fn tag(id: &str, kind: i32, name: &str) -> crate::library::db::TagRow {
    crate::library::db::TagRow {
        id: id.into(),
        name: name.into(),
        kind,
        count: 0,
    }
}

#[test]
fn count_is_computed_live_on_every_read() {
    let db = test_db("tag_count_live");
    crate::library::db::tag::insert_tag(&db, &tag("t1", 1, "music"));
    let before = crate::library::db::tag::tag_by_id(&db, "t1").unwrap();
    assert_eq!(before.count, 0);
    crate::library::db::tag::insert_relations(
        &db,
        &[("t1".into(), "k1".into()), ("t1".into(), "k2".into())],
    );
    let after = crate::library::db::tag::tag_by_id(&db, "t1").unwrap();
    assert_eq!(after.count, 2);
    // There is no stored count column to drift — the subquery is the value.
    let raw: i64 = db.with_conn(|c| {
        c.query_row(
            "SELECT COUNT(*) FROM tag_relations WHERE tag_id='t1'",
            [],
            |r| r.get(0),
        )
        .unwrap()
    });
    assert_eq!(raw, 2);
}

#[test]
fn remove_relations_cross_product() {
    let db = test_db("tag_cross");
    crate::library::db::tag::insert_tag(&db, &tag("t1", 1, "a"));
    crate::library::db::tag::insert_tag(&db, &tag("t2", 1, "b"));
    crate::library::db::tag::insert_relations(
        &db,
        &[
            ("t1".into(), "k1".into()),
            ("t1".into(), "k2".into()),
            ("t2".into(), "k1".into()),
        ],
    );
    // Remove only (t1 × k1): (t1,k2) and (t2,k1) survive.
    crate::library::db::tag::remove_relations(&db, &["k1".to_string()], &["t1".to_string()]);
    let t1_keys = crate::library::db::tag::keys_for_tag(&db, "t1");
    assert_eq!(t1_keys, vec!["k2".to_string()]);
    let t2_keys = crate::library::db::tag::keys_for_tag(&db, "t2");
    assert_eq!(t2_keys, vec!["k1".to_string()]);
}
