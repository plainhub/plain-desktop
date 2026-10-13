use super::*;

#[test]
fn audit_filter_is_unicode_case_insensitive_before_pagination_and_sql_errors_are_real() {
    let db = Db::open(std::path::Path::new(":memory:")).unwrap();
    let older = db.audit_event_add("login", "ÄBC", "client").unwrap();
    let newer = db.audit_event_add("mount", "other", "client").unwrap();
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE audit_events SET created_at='2026-10-13T00:00:00.000Z' WHERE id=?1",
            [&older],
        )
    })
    .unwrap();
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE audit_events SET created_at='2026-10-13T01:00:00.000Z' WHERE id=?1",
            [&newer],
        )
    })
    .unwrap();
    assert_eq!(db.audit_events(0, 1, None).unwrap()[0].id, newer);
    assert_eq!(db.audit_events(0, 1, Some("äbc")).unwrap()[0].id, older);
    assert!(db.audit_events(1, 1, Some("äbc")).unwrap().is_empty());
    db.with_conn(|conn| conn.execute_batch("DROP TABLE audit_events"))
        .unwrap();
    assert!(db.audit_events(0, 1, None).is_err());
}
