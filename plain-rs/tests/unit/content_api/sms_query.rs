use super::*;
#[test]
fn archived_conversation_search_matches_address_and_snippet_case_insensitively() {
    let addresses = vec!["+1 555 0100".to_owned()];
    assert!(archived_matches(
        &addresses,
        "Old project update",
        "555 0100"
    ));
    assert!(archived_matches(
        &addresses,
        "Old project update",
        "PROJECT"
    ));
    assert!(archived_matches(&addresses, "Old project update", "  "));
    assert!(!archived_matches(
        &addresses,
        "Old project update",
        "missing"
    ));
}

fn db(dir: &tempfile::TempDir) -> Db {
    Db::open(&dir.path().join("db")).unwrap()
}
#[test]
fn sms_mms_plans_partition_ids_literal_text_trash_and_archive_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    let db = db(&dir);
    db.archived_conversation_save(&crate::db::ArchivedConversationRow {
        conversation_id: "4".into(),
        conversation_date: "2026-10-05T01:00:00.123Z".into(),
    })
    .unwrap();
    db.trashed_message_save_many(&[
        crate::db::TrashedMessageRow {
            message_id: "mms_2".into(),
            is_mms: true,
            trashed_at: "today".into(),
        },
        crate::db::TrashedMessageRow {
            message_id: "1".into(),
            is_mms: false,
            trashed_at: "today".into(),
        },
    ])
    .unwrap();
    let fields =
        provider_plan::fields(&db, "thread_id:4 archived:1 ids:1,mms_2 text:'%_'").unwrap();
    let result = plans(&db, &fields, false, Some(vec!["2".into()])).unwrap();
    assert!(result.sms.clauses.contains(&"date <= ?".into()));
    assert!(result.sms.args.contains(&"\\%\\_".into()));
    assert_eq!(result.sms.args.last().unwrap(), "1791162000123");
    let mms = result.mms.unwrap();
    assert_eq!(mms.args.last().unwrap(), "1791162000");
    assert!(!mms.clauses.join(" ").contains("mms_"));
    assert!(mms.clauses.contains(&"m_type IN (128,132)".into()));
    let fields = provider_plan::fields(&db, "ids:mms_bad").unwrap();
    let result = plans(&db, &fields, false, None).unwrap();
    assert!(result.mms.is_none());
    assert!(result.sms.args.contains(&"-1".into()));
}
#[test]
fn merged_messages_sort_real_instants_then_page_and_resolve_blank_addresses() {
    let items = vec![
        json!({"id":"sms","date":"2026-10-05T01:00:00Z","address":""}),
        json!({"id":"mms","date":"2026-10-05T04:00:00+02:00","address":"123"}),
    ];
    let result = page(items, 1, 1, "canonical").unwrap();
    assert_eq!(result[0]["id"], "sms");
    assert_eq!(result[0]["address"], "canonical");
    assert_eq!(
        text_ids(
            json!({"1":["Hello","World"],"2":["hello wrong"]}),
            &["hello".into(), "world".into()]
        )
        .unwrap(),
        vec!["1"]
    );
    assert!(page(vec![json!({"date":"bad"})], 0, 1, "").is_err());
}
