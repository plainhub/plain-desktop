use super::*;
#[path = "fixtures.rs"]
mod fixtures;
use crate::library::tags;
use fixtures::test_db;
fn input(id: &str, kind: i32, key: &str) -> RelationInput {
    RelationInput {
        tag_id: id.into(),
        key: key.into(),
        kind,
        size_bytes: 5_000_000_001,
        title: "测试, 'title'".into(),
    }
}
#[test]
fn typed_keys_and_metadata_survive_edits() {
    let db = test_db("records_metadata");
    let audio = tags::create_tag(&db, 1, "audio").unwrap();
    let image = tags::create_tag(&db, 3, "image").unwrap();
    let key = "/a, 'b' 图片";
    add(&db, &[input(&audio.id, 1, key), input(&image.id, 3, key)]).unwrap();
    let first = relations(&db, 1, &[key.into()]).unwrap().remove(0);
    edit(&db, 1, key, "new", 6_000_000_001, &[audio.id.clone()], &[]).unwrap();
    let second = relations(&db, 1, &[key.into()]).unwrap().remove(0);
    assert_eq!(first.created_at, second.created_at);
    assert_eq!(second.title, "new");
    assert_eq!(second.size_bytes, 6_000_000_001);
    remove_keys(&db, 1, &[key.into()]).unwrap();
    assert!(relations(&db, 1, &[key.into()]).unwrap().is_empty());
    assert_eq!(relations(&db, 3, &[key.into()]).unwrap().len(), 1);
    assert_eq!(get(&db, &audio.id).unwrap().unwrap().count, 0);
    assert_eq!(get(&db, &image.id).unwrap().unwrap().count, 1);
}
#[test]
fn cross_kind_batch_failure_rolls_back_metadata() {
    let db = test_db("records_batch");
    let a = tags::create_tag(&db, 1, "a").unwrap();
    let b = tags::create_tag(&db, 3, "b").unwrap();
    add(&db, &[input(&a.id, 1, "key")]).unwrap();
    let mut changed = input(&a.id, 1, "key");
    changed.title = "changed".into();
    assert!(add(&db, &[changed, input(&b.id, 1, "other")]).is_err());
    assert_eq!(
        relations(&db, 1, &["key".into()]).unwrap()[0].title,
        "测试, 'title'"
    );
    assert_eq!(get(&db, &b.id).unwrap().unwrap().count, 0);
}
#[test]
fn intersection_deduplicates_requested_ids_and_checks_types() {
    let db = test_db("records_intersection");
    let a = tags::create_tag(&db, 1, "a").unwrap();
    let b = tags::create_tag(&db, 1, "b").unwrap();
    let other = tags::create_tag(&db, 3, "c").unwrap();
    add(
        &db,
        &[
            input(&a.id, 1, "both"),
            input(&b.id, 1, "both"),
            input(&a.id, 1, "only"),
        ],
    )
    .unwrap();
    assert_eq!(
        intersection(&db, &[a.id.clone(), b.id.clone(), a.id.clone()]).unwrap(),
        ["both"]
    );
    assert!(intersection(&db, &[a.id, other.id]).is_err());
    assert!(intersection(&db, &[]).unwrap().is_empty());
    assert!(intersection(&db, &["missing".into()]).unwrap().is_empty());
}
