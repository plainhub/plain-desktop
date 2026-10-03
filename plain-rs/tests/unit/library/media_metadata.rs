use super::*;
#[path = "fixtures.rs"]
mod fixtures;
use fixtures::test_db;
#[test]
fn exact_milliseconds_and_typed_identity() {
    let db = test_db("metadata_identity");
    save(&db, "audio", "same", 5_000_000_001).unwrap();
    save(&db, "video", "same", 6_000_000_001).unwrap();
    let rows = all(&db).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].duration_ms, 5_000_000_001);
    assert!(save(&db, "video", "same", -1).is_err());
    assert_eq!(delete(&db, "audio", &[]).unwrap(), 0);
    assert_eq!(delete(&db, "audio", &["same".into()]).unwrap(), 1);
    assert_eq!(all(&db).unwrap()[0].duration_ms, 6_000_000_001);
}
