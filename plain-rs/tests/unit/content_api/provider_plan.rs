use super::*;
#[test]
fn provider_plans_bind_text_ids_and_only_closed_comparisons() {
    let fields =
        search_dsl::parse("text:'50%_\\\\' ids:1,2 type:3 duration:>=60 start_time:>2026-10-05");
    let result = plan(Provider::Call, &fields, &json!({"2026-10-05":1234}));
    assert!(result.clauses.iter().any(|s| s == "duration >= ?"));
    assert!(result.clauses.iter().any(|s| s == "date > ?"));
    assert_eq!(result.args.last().unwrap(), "1234");
    assert!(!result.clauses.join(" ").contains("50"));
    assert!(result.args[0].contains("\\%\\_"));
    let malicious = vec![FilterField {
        name: "duration".into(),
        op: "OR 1=1".into(),
        value: "1".into(),
    }];
    assert!(
        super::plan(Provider::Call, &malicious, &Value::Null)
            .clauses
            .is_empty()
    );
}
#[test]
fn contact_plans_keep_name_rows_and_empty_tag_ids_match_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("db")).unwrap();
    let parsed = fields(&db, "tag_id:absent").unwrap();
    let result = plan(Provider::Contact, &parsed, &Value::Null);
    assert_eq!(
        result.args,
        vec!["vnd.android.cursor.item/name", "invalid_ids"]
    );
    assert!(result.clauses.iter().any(|s| s == "raw_contact_id IN (?)"));
}
