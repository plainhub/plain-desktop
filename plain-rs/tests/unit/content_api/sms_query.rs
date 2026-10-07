use super::*;
use crate::content_api::host::Host;
use std::sync::Arc;
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

/// The path that turns a query into the id set `trashSms` / `restoreSms` /
/// `deleteSms` act on used to drop every unrecognised field, which left the
/// clause list empty, and `ContentWhere` renders an empty clause list as
/// `1=1` — so a misspelt filter covered the whole table.
#[test]
fn a_destructive_query_that_builds_no_clause_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("db")).unwrap();

    // Every spelling of "names nothing that narrows": a misspelt field, the
    // names that merely travel along from a media view, and `archived` — which
    // only contributes a clause when `thread_id` also matches a stored archive
    // record, so on its own it builds nothing. A list of field names would call
    // the last one a selector and let the whole table through.
    for query in [
        "zzz_no_such_field:x",
        "trash:false",
        "bucket_id:7",
        "archived:1",
    ] {
        let fields = provider_plan::fields(&db, query).unwrap();
        let plans = plans(&db, &fields, true, None).unwrap();
        assert!(
            plans.sms.clauses.is_empty(),
            "{query:?} was expected to build no clause at all"
        );
        assert!(
            narrows_nothing(query, &fields, &plans.sms),
            "{query:?} would reach deleteSms with nothing selected, and the whole table is in \
             range"
        );
    }

    // The fields that do build a clause, so a real selection goes through.
    for query in ["ids:1,2", "text:hello", "type:1", "thread_id:7", "trashed:1"] {
        let fields = provider_plan::fields(&db, query).unwrap();
        let plans = plans(&db, &fields, true, None).unwrap();
        assert!(
            !plans.sms.clauses.is_empty(),
            "{query:?} was expected to build a clause"
        );
        assert!(!narrows_nothing(query, &fields, &plans.sms));
    }
    // `archived` next to the `thread_id` it needs is narrowed by that thread id.
    let paired = provider_plan::fields(&db, "archived:1 thread_id:7").unwrap();
    assert!(!narrows_nothing(
        "archived:1 thread_id:7",
        &paired,
        &plans(&db, &paired, true, None).unwrap().sms
    ));

    // A misspelt name beside a real one costs precision, not the whole table.
    // That is why the guard reads the plan instead of refusing unknown names
    // outright — a messages query carries `trash` and `bucket_id` over from a
    // media view, and refusing those would break search.
    let mixed = provider_plan::fields(&db, "zzz_no_such_field:x ids:1").unwrap();
    assert!(!narrows_nothing(
        "zzz_no_such_field:x ids:1",
        &mixed,
        &plans(&db, &mixed, true, None).unwrap().sms
    ));

    // The two queries allowed to select everything, because they say so.
    for query in ["", "all:true"] {
        let fields = provider_plan::fields(&db, query).unwrap();
        let plans = plans(&db, &fields, true, None).unwrap();
        assert!(plans.sms.clauses.is_empty());
        assert!(!narrows_nothing(query, &fields, &plans.sms));
    }
}

/// The predicate above is only worth anything if `prepare` asks it, and
/// `prepare` is only reached with `destructive` set from the `Ids` arm — the
/// one `trashSms` / `restoreSms` / `deleteSms` resolve their ids through. So
/// this drives the real request type and watches both what the platform is
/// asked for and what comes back.
#[tokio::test]
async fn the_ids_request_refuses_a_whole_table_selection_before_asking_the_platform() {
    fn stub(host: Arc<Host>, ids: Vec<String>) {
        let (generation, mut requests) = host.connect();
        tokio::spawn(async move {
            while let Some(request) = requests.recv().await {
                let Some(id) = request["id"].as_u64() else {
                    continue;
                };
                let result = match request["method"].as_str().unwrap_or_default() {
                    "systemSmsIdsFacts" => json!(ids),
                    other => panic!("unexpected host call {other}"),
                };
                let _ = host.reply(generation, json!({"id": id, "result": result}));
            }
        });
    }

    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("db")).unwrap();
    let host = Arc::new(Host::default());
    stub(host.clone(), vec!["1".to_owned(), "2".to_owned()]);

    let error = execute(
        &db,
        &host,
        Request::Ids {
            query: "archived:1".into(),
            include_trashed: true,
        },
    )
    .await
    .expect_err("a query that builds no clause must not reach the platform");
    assert!(
        error.to_string().contains("whole table"),
        "the refusal has to say why, got: {error}"
    );

    // A real selection still resolves, and the platform sees exactly the ids
    // the clause picked out.
    let resolved = execute(
        &db,
        &host,
        Request::Ids {
            query: "ids:7".into(),
            include_trashed: true,
        },
    )
    .await
    .unwrap();
    assert_eq!(resolved, json!({"ids": ["1", "2"]}));
}
