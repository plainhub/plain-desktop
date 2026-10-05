use super::*;
#[test]
fn provider_plans_bind_text_ids_and_only_closed_comparisons() {
    let fields =
        search_dsl::parse("text:'50%_\\\\' ids:1,2 type:3 duration:>=60 start_time:>2026-10-05");
    let result = plan(
        Provider::Call,
        &fields,
        &json!({"2026-10-05":1234}),
        None,
        false,
    );
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
        super::plan(Provider::Call, &malicious, &Value::Null, None, false)
            .clauses
            .is_empty()
    );
}
#[test]
fn contact_plans_keep_name_rows_and_empty_tag_ids_match_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("db")).unwrap();
    let parsed = fields(&db, "tag_id:absent").unwrap();
    let result = plan(Provider::Contact, &parsed, &Value::Null, None, false);
    assert_eq!(result.args, vec!["vnd.android.cursor.item/name"]);
    assert_eq!(result.ids_column.as_deref(), Some("raw_contact_id"));
    assert_eq!(result.ids, vec!["invalid_ids"]);
}

#[test]
fn media_plans_own_filters_and_keep_values_bound() {
    let fields = vec![
        FilterField {
            name: "text".into(),
            op: ":".into(),
            value: "%_\\".into(),
        },
        FilterField {
            name: "excluded_dir".into(),
            op: ":".into(),
            value: "/storage/Pictures".into(),
        },
        FilterField {
            name: "file_size".into(),
            op: ":".into(),
            value: ">=1.5 MB".into(),
        },
        FilterField {
            name: "trash".into(),
            op: ":".into(),
            value: "true".into(),
        },
    ];
    let result = plan(Provider::Doc, &fields, &Value::Null, None, false);
    assert_eq!(result.trash, Some(true));
    assert!(result.clauses.iter().any(|c| c.contains("mime_type LIKE")));
    assert!(result.clauses.iter().any(|c| c == "size >= ?"));
    assert!(result.args.contains(&1_572_864u64.to_string()));
    assert!(result.args.contains(&"\\%\\_\\\\".to_owned()));
    assert!(
        result
            .clauses
            .iter()
            .all(|c| !c.contains("/storage/Pictures"))
    );
}

#[test]
fn plain_file_plans_keep_parent_facts_host_owned_and_hide_dotfiles_by_default() {
    let fields =
        search_dsl::parse("text:'report%_' parent:/storage/docs file_size:>=1.5MB ids:7,8");
    let result = plan(Provider::File, &fields, &Value::Null, Some("42"), false);
    assert!(
        result
            .clauses
            .iter()
            .any(|c| c == "_display_name NOT LIKE ? ESCAPE '\\'")
    );
    assert!(result.clauses.iter().any(|c| c == "parent = ?"));
    assert!(
        result
            .clauses
            .iter()
            .any(|c| c == "_display_name LIKE '%' || ? || '%' ESCAPE '\\'")
    );
    assert!(result.clauses.iter().any(|c| c == "size >= ?"));
    assert!(result.args.contains(&"42".to_owned()));
    assert!(result.args.contains(&1_572_864u64.to_string()));
    assert!(result.args.contains(&"report\\%\\_".to_owned()));
    let wire = serde_json::to_value(&result).unwrap();
    assert_eq!(wire["idsColumn"], "_id");
    assert!(wire.get("ids_column").is_none());

    let shown = search_dsl::parse("show_hidden:true");
    assert!(
        !plan(Provider::File, &shown, &Value::Null, None, false)
            .clauses
            .iter()
            .any(|c| c.contains("NOT LIKE"))
    );
}
