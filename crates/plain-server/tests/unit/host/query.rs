use super::*;
use crate::host::bridge::Host;

/// `plan` now rejects a field the provider does not know, so the cases below —
/// which are all about what a *known* field produces — unwrap in one place.
fn plan_ok(
    provider: Provider,
    fields: &[FilterField],
    dates: &Value,
    resolved_parent_id: Option<&str>,
    query_is_empty: bool,
) -> Plan {
    plan(provider, fields, dates, resolved_parent_id, query_is_empty)
        .expect("every field in this test is one the provider knows")
}
#[test]
fn the_shared_planner_refuses_the_providers_the_mobile_one_owns() {
    // `provider_query` routes Call and Contact to `mobile::query::plan` under
    // the `mobile` feature, so this planner must refuse their vocabulary
    // rather than half-implementing it.
    for provider in [Provider::Call, Provider::Contact] {
        for query in ["id:1", "text:1", "ids:1"] {
            assert!(
                plan(
                    provider,
                    &search_dsl::parse(query),
                    &Value::Null,
                    None,
                    false
                )
                .is_err(),
                "{provider:?} must not plan `{query}` here"
            );
        }
    }
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
    let result = plan_ok(Provider::Doc, &fields, &Value::Null, None, false);
    assert_eq!(result.trash, Some(true));
    assert!(result.clauses.iter().any(|c| c.contains("mime_type LIKE")));
    assert!(result.clauses.iter().any(|c| c == "_size >= ?"));
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
    let result = plan_ok(Provider::File, &fields, &Value::Null, Some("42"), false);
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
    assert!(result.clauses.iter().any(|c| c == "_size >= ?"));
    assert!(result.args.contains(&"42".to_owned()));
    assert!(result.args.contains(&1_572_864u64.to_string()));
    assert!(result.args.contains(&"report\\%\\_".to_owned()));
    let wire = serde_json::to_value(&result).unwrap();
    assert_eq!(wire["idsColumn"], "_id");
    assert!(wire.get("ids_column").is_none());

    let shown = search_dsl::parse("show_hidden:true");
    assert!(
        !plan_ok(Provider::File, &shown, &Value::Null, None, false)
            .clauses
            .iter()
            .any(|c| c.contains("NOT LIKE"))
    );
}

#[test]
fn mediastore_plans_only_name_columns_its_strict_grammar_accepts() {
    // MediaProvider parses the legacy /file URI with a strict SQL grammar and
    // answers an unknown identifier with `Invalid token <name>`, which reaches
    // the app as a fatal IllegalArgumentException. `_size` is the real column;
    // the bare `size` alias is not part of the grammar.
    const COLUMNS: &[&str] = &[
        "_data",
        "_display_name",
        "_id",
        "_size",
        "artist",
        "bucket_id",
        "data2",
        "date",
        "date_added",
        "date_modified",
        "duration",
        "media_type",
        "mime_type",
        "mimetype",
        "number",
        "parent",
        "raw_contact_id",
        "title",
        "type",
    ];
    const KEYWORDS: &[&str] = &["and", "or", "not", "like", "in", "escape", "is"];

    // One query per provider, holding only the fields that provider knows.
    // `plan` refuses a field it has no column for, so a single shared soup of
    // every field would test the refusal rather than the grammar — and the
    // refusal has its own test below.
    let queries: &[(Provider, &str)] = &[
        (Provider::Doc, ""),
        (Provider::Doc, "text:'a%_'"),
        (
            Provider::Doc,
            "file_size:>=1.5MB ext:pdf type:text/plain bucket_id:1 parent:/sd \
             excluded_dir:/sd/x show_hidden:true ids:1,2 trash:true root_path:/sd",
        ),
        (Provider::File, ""),
        (Provider::File, "text:'a%_'"),
        (
            Provider::File,
            "file_size:>=1.5MB type:text/plain parent:/sd root_path:/sd \
             show_hidden:true ids:1,2 trash:true all:true",
        ),
        (Provider::Audio, ""),
        (Provider::Audio, "text:'a%_'"),
        (
            Provider::Audio,
            "type:image/png bucket_id:1 excluded_dir:/sd/x show_hidden:true \
             ids:1,2 trash:true",
        ),
        (
            Provider::Video,
            "text:'a%_' bucket_id:1 ids:1,2 trash:true type:video/mp4",
        ),
        (
            Provider::Image,
            "text:'a%_' bucket_id:1 ids:1,2 trash:true type:image/png",
        ),
    ];

    for (provider, query) in queries {
        {
            let fields = search_dsl::parse(query);
            let result = plan_ok(*provider, &fields, &Value::Null, Some("1"), false);
            for clause in &result.clauses {
                let stripped: String = {
                    let mut out = String::with_capacity(clause.len());
                    let mut quoted = false;
                    for c in clause.chars() {
                        match c {
                            '\'' => quoted = !quoted,
                            _ if !quoted => out.push(c),
                            _ => {}
                        }
                    }
                    out
                };
                for token in stripped.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
                    if token.is_empty() || token.starts_with(|c: char| c.is_ascii_digit()) {
                        continue;
                    }
                    assert!(
                        COLUMNS.contains(&token)
                            || KEYWORDS.contains(&token.to_ascii_lowercase().as_str()),
                        "{provider:?} clause `{clause}` names unknown column `{token}`"
                    );
                }
            }
        }
    }
}

/// Every name a `plain-desktop` query builder can emit must still be accepted
/// for the providers it can reach, because those are the queries that work
/// today and `plan` now refuses anything else. The source of each row is the
/// builder that produces it; adding a filter key to one of them without adding
/// the matching `intentionally_ignored` arm fails here by name.
#[test]
fn every_field_the_web_client_can_emit_is_still_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("db")).unwrap();
    let cases: &[(Provider, &[&str], &str)] = &[
        // hooks/search.ts buildQ + hooks/header-search-query.ts buildNextMediaQ,
        // with the keys offered by components/header-search/options.ts.
        (
            Provider::Audio,
            &["text", "tag_id", "bucket_id", "trash", "type"],
            "media view",
        ),
        (
            Provider::Video,
            &["text", "tag_id", "bucket_id", "trash"],
            "media view",
        ),
        (
            Provider::Image,
            &["text", "tag_id", "bucket_id", "trash"],
            "media view",
        ),
        // hooks/files.ts buildQ, plus root_path from hooks/files-sidebar.ts.
        (
            Provider::File,
            &[
                "text",
                "parent",
                "type",
                "root_path",
                "show_hidden",
                "file_size",
            ],
            "files view",
        ),
        // buildNextDocsQ keeps whatever the shared files filter already holds,
        // so a docs query can arrive carrying any of the files keys too.
        (
            Provider::Doc,
            &[
                "text",
                "ext",
                "file_size",
                "parent",
                "type",
                "root_path",
                "show_hidden",
                "trash",
            ],
            "docs view",
        ),
    ];

    for (provider, names, origin) in cases {
        for name in *names {
            // Through `fields`, so `tag_id` takes the real path where it is
            // rewritten into `ids` before `plan` ever sees the name.
            let query = format!("{name}:__plain_probe__");
            let parsed = fields(&db, &query).unwrap();
            plan(*provider, &parsed, &Value::Null, Some("1"), false).unwrap_or_else(|e| {
                panic!("{origin} can send `{name}` to {provider:?} but plan refused it: {e}")
            });
        }
    }
}

#[test]
fn media_trash_rides_the_query_argument_and_builds_no_clause() {
    // The phone trashes through MediaStore, and MediaStore answers a
    // trashed-only read with the `QUERY_ARG_MATCH_TRASHED` argument. None of
    // that is a selection, so the plan carries the flag alone. It must build no
    // clause: a clause here would be a path match on the NAS store's
    // `.plain-trash` tree, which no phone process creates, and `trash:false`
    // producing one is exactly how a bulk ask read as narrowed when it was
    // really the whole table.
    for provider in [
        Provider::Doc,
        Provider::Audio,
        Provider::Video,
        Provider::Image,
    ] {
        // What the same provider builds with nothing in the query at all: Doc
        // pins its mime types, the rest build nothing. Trash must add to that
        // baseline rather than to it.
        let bare = plan_ok(provider, &search_dsl::parse(""), &Value::Null, None, false);
        for (query, expected) in [("trash:true", true), ("trash:false", false)] {
            let plan = plan_ok(
                provider,
                &search_dsl::parse(query),
                &Value::Null,
                None,
                false,
            );
            assert_eq!(
                plan.trash,
                Some(expected),
                "{provider:?} {query} lost the flag"
            );
            assert_eq!(
                plan.clauses, bare.clauses,
                "{provider:?} {query} built a selection clause: {:?}",
                plan.clauses
            );
        }
    }
}

/// A field the provider has never heard of is refused by name. Dropping it
/// used to leave the clause list empty, which ContentWhere renders as `1=1` —
/// so a bulk delete naming a misspelt field hit the whole table.
#[test]
fn unknown_fields_are_refused_instead_of_dropped() {
    for provider in [
        Provider::File,
        Provider::Doc,
        Provider::Audio,
        Provider::Image,
        Provider::Video,
    ] {
        // A name no provider has an arm for. `id` would not do: it is a real
        // Contact field, and this test's job is to catch names that are not.
        // Plan has no Debug, so expect_err would not compile here.
        let err = plan(
            provider,
            &search_dsl::parse("zzz_no_such_field:__probe__"),
            &Value::Null,
            None,
            false,
        )
        .err()
        .unwrap_or_else(|| panic!("zzz_no_such_field matched a {provider:?} arm"));
        assert!(
            err.to_string().contains("zzz_no_such_field"),
            "{provider:?} should name the field, said: {err}"
        );
    }
    // The whole-table sentinel and the no-op flags still pass on every
    // provider: rejecting them would break queries that work today.
    for query in ["all:true", "trash:false", "show_hidden:true"] {
        for provider in [Provider::File, Provider::Doc, Provider::Audio] {
            plan(
                provider,
                &search_dsl::parse(query),
                &Value::Null,
                None,
                false,
            )
            .unwrap_or_else(|e| panic!("{query} must stay accepted for {provider:?}: {e}"));
        }
    }
}

/// The four providers that contribute no clause of their own could resolve a
/// query of fields they do not act on to every row, and an empty clause list is
/// what `ContentWhere` renders as `1=1`. The predicate asks the built plan, so
/// these cases must agree with what `plan` actually produced — if they drift,
/// the guard is guarding a fiction.
#[tokio::test]
async fn a_query_that_builds_no_clause_is_the_one_the_destructive_path_refuses() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("db")).unwrap();
    // No `start_time` in any query below, so the host is never called.
    let host = Host::default();

    // The same field name, two providers, opposite answers: `type` is a Call
    // column and a no-op on the media providers, whose own kind is the provider
    // choice. A guard written against field names would get one of these two
    // wrong.
    for (provider, query) in [
        (Provider::Audio, "type:1"),
        (Provider::Audio, "show_hidden:false"),
        (Provider::Image, "type:1"),
        (Provider::Video, "type:1"),
        // The complement of the trashed set is the whole table.
        (Provider::Audio, "trash:false"),
        (Provider::Image, "trash:false"),
    ] {
        let parsed = fields(&db, query).unwrap();
        assert!(
            plan(provider, &parsed, &Value::Null, None, query.is_empty())
                .unwrap()
                .clauses
                .is_empty(),
            "{provider:?} {query:?} was expected to build no clause"
        );
        assert!(
            require_narrowing(&db, &host, provider, query)
                .await
                .is_err(),
            "{provider:?} {query:?} reaches a mutation with nothing selected"
        );
    }

    for (provider, query) in [
        (Provider::Audio, "text:x"),
        // Always narrowed by a clause of the provider's own, so the guard can
        // never fire for them however the query is spelled.
        (Provider::Doc, "trash:false"),
        (Provider::File, "trash:false"),
    ] {
        require_narrowing(&db, &host, provider, query)
            .await
            .unwrap_or_else(|e| panic!("{provider:?} {query:?} was refused: {e}"));
    }

    // `all` builds no clause by design and says so; blank is the caller's own
    // guard to own. So does `trash:true`, which names the trashed set.
    require_narrowing(&db, &host, Provider::Audio, "all:true")
        .await
        .expect("all:true is the sanctioned whole-table ask");
    require_narrowing(&db, &host, Provider::Audio, "trash:true")
        .await
        .expect("trash:true is the trashed set, which is a scope of its own");
    require_narrowing(&db, &host, Provider::Audio, "")
        .await
        .expect("blank is refused by the caller's guard, not this one");
}
