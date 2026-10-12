//! Contact and Call query planning. The shared media planner refuses both
//! providers — `provider_query` routes them here under the `mobile` feature
//! — so their vocabulary is asserted against this planner, not the media one.

use super::*;
use crate::host::query::fields;
use plain_rs::db::Db;
use serde_json::Value;

fn plan_ok(provider: Provider, fields: &[FilterField], dates: &Value) -> anyhow::Result<Plan> {
    plan(provider, fields, dates)
}

#[test]
fn contact_plans_keep_name_rows_and_empty_tag_ids_match_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("db")).unwrap();
    let parsed = fields(&db, "tag_id:absent").unwrap();
    let result = plan_ok(Provider::Contact, &parsed, &Value::Null).unwrap();
    assert_eq!(result.args, vec!["vnd.android.cursor.item/name"]);
    assert_eq!(result.ids_column.as_deref(), Some("raw_contact_id"));
    assert_eq!(result.ids, vec!["invalid_ids"]);
}

#[test]
fn contact_and_call_own_disjoint_field_vocabulary() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("db")).unwrap();
    // `id` is a contact's own field and no other provider's.
    assert!(
        plan_ok(
            Provider::Contact,
            &fields(&db, "id:1").unwrap(),
            &Value::Null
        )
        .is_ok()
    );
    assert!(
        plan_ok(Provider::Call, &fields(&db, "id:1").unwrap(), &Value::Null).is_err(),
        "Contact's `id` must not leak into the call vocabulary"
    );
}

#[test]
fn the_web_clients_contact_and_call_fields_are_still_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("db")).unwrap();
    let cases: &[(Provider, &[&str])] = &[
        // Mirrors the calls view, `buildNextCallsQ`.
        (Provider::Call, &["text", "type", "duration", "start_time"]),
        // The contacts view sends query:""; these two exist for other callers.
        (Provider::Contact, &["text", "ids", "id"]),
    ];
    for (provider, names) in cases {
        for name in *names {
            let parsed = fields(&db, &format!("{name}:__plain_probe__")).unwrap();
            plan_ok(*provider, &parsed, &Value::Null).unwrap_or_else(|error| {
                panic!("{provider:?} must accept `{name}`, refused: {error}")
            });
        }
    }
}

#[test]
fn unknown_provider_fields_are_named_rather_than_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("db")).unwrap();
    // A name neither provider has an arm for. `type` and `id` would not do:
    // each belongs to exactly one of them.
    for provider in [Provider::Call, Provider::Contact] {
        let error = plan_ok(
            provider,
            &plain_rs::utils::search_dsl::parse("zzz_no_such_field:1"),
            &Value::Null,
        )
        .err()
        .unwrap_or_else(|| panic!("{provider:?} accepted an unknown field"));
        assert!(
            error.to_string().contains("zzz_no_such_field"),
            "{provider:?} should name the field, said: {error}"
        );
    }
}

#[test]
fn call_duration_and_start_time_bind_only_closed_comparisons() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("db")).unwrap();
    let filter = |name: &str| {
        vec![FilterField {
            name: name.into(),
            op: ":".into(),
            value: "__plain_probe__".into(),
        }]
    };
    // A bare `:` is not a closed comparison, so it contributes no clause.
    let result = plan_ok(Provider::Call, &filter("duration"), &Value::Null).unwrap();
    assert!(result.clauses.is_empty(), "{:?}", result.clauses);
    let result = plan_ok(Provider::Call, &filter("start_time"), &Value::Null).unwrap();
    assert!(result.clauses.is_empty(), "{:?}", result.clauses);
}

#[test]
fn whole_table_sentinels_pass_on_every_provider() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("db")).unwrap();
    for (provider, query) in [
        (Provider::Call, "trash:false"),
        (Provider::Call, "show_hidden:false"),
        (Provider::Contact, "trash:false"),
    ] {
        plan_ok(provider, &fields(&db, query).unwrap(), &Value::Null)
            .unwrap_or_else(|error| panic!("{query} must pass on {provider:?}: {error}"));
    }
}
#[test]
fn call_plans_bind_text_ids_and_only_closed_comparisons() {
    let parsed = plain_rs::utils::search_dsl::parse(
        "text:'50%_\\\\' ids:1,2 type:3 duration:>=60 start_time:>2026-10-05",
    );
    let result = plan_ok(
        Provider::Call,
        &parsed,
        &serde_json::json!({"2026-10-05": 1234}),
    )
    .unwrap();
    assert!(
        result
            .clauses
            .iter()
            .any(|clause| clause == "duration >= ?")
    );
    assert!(result.clauses.iter().any(|clause| clause == "date > ?"));
    assert_eq!(result.args.last().unwrap(), "1234");
    assert!(!result.clauses.join(" ").contains("50"));
    assert!(result.args[0].contains("\\%\\_"));
}

#[test]
fn call_rejects_an_injected_comparison_operator() {
    let malicious = vec![FilterField {
        name: "duration".into(),
        op: "OR 1=1".into(),
        value: "1".into(),
    }];
    let result = plan_ok(Provider::Call, &malicious, &Value::Null).unwrap();
    assert!(result.clauses.is_empty(), "{:?}", result.clauses);
}

#[tokio::test]
async fn call_and_contact_reach_a_mutation_with_nothing_selected() {
    use crate::host::bridge::Host;
    use std::sync::Arc;

    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(Db::open(&dir.path().join("db")).unwrap());
    let host = Arc::new(Host::default());
    for (provider, query) in [
        (Provider::Call, "trash:false"),
        (Provider::Call, "show_hidden:false"),
    ] {
        assert!(
            super::require_narrowing(&db, &host, provider, query)
                .await
                .is_err(),
            "{provider:?} {query:?} selects nothing and must refuse the mutation"
        );
    }
    // The contact plan pins its own `mimetype` clause, so every contact query
    // narrows even when the sent field contributes nothing.
    for (provider, query) in [
        (Provider::Call, "type:1"),
        (Provider::Contact, "trash:false"),
        (Provider::Contact, "text:abc"),
    ] {
        super::require_narrowing(&db, &host, provider, query)
            .await
            .unwrap_or_else(|error| panic!("{provider:?} {query:?} always narrows: {error}"));
    }
}
