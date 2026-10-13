use super::*;

#[test]
fn custom_bearer_requires_the_exact_second_header_token_and_a_custom_session() {
    assert!(valid_custom_bearer("Bearer secret", "secret"));
    assert!(!valid_custom_bearer("Bearer wrong", "secret"));
    assert!(!valid_custom_bearer("Bearer secret", ""));
    assert!(!valid_custom_bearer("Bearer", "secret"));
    assert!(!valid_custom_bearer("secret", "secret"));
    assert!(!valid_custom_bearer("Bearer  secret", "secret"));
}

#[test]
fn shutdown_only_accepts_ipv4_and_ipv6_loopback_peers() {
    assert!(shutdown_allowed("127.0.0.1:443".parse().unwrap()));
    assert!(shutdown_allowed("[::1]:443".parse().unwrap()));
    assert!(!shutdown_allowed("192.168.1.2:443".parse().unwrap()));
    assert!(!shutdown_allowed("[::ffff:127.0.0.1]:443".parse().unwrap()));
}

fn contract_schema() -> (
    tempfile::TempDir,
    crate::graphql::schemas::public::PublicSchema,
) {
    use plain_rs::db::Db;
    use plain_rs::prefs::Prefs;
    use std::sync::Arc;
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(Prefs::load(&dir.path().join("prefs.json")).unwrap());
    let db = Arc::new(Db::open(&dir.path().join("data.db")).unwrap());
    let (events, _) = tokio::sync::broadcast::channel(16);
    let directory = dir.path().to_path_buf();
    (
        dir,
        crate::graphql::schemas::public::build(
            Arc::new(crate::host::bridge::Host::default()),
            events,
            prefs,
            db,
            directory,
        ),
    )
}

/// The flip itself: the public `/graphql` answers from the Rust contract
/// schema. Before it, this document went to the platform as
/// `mainGraphqlExecute`; a query that only touches Rust state therefore
/// proves no such hop is left on the path.
#[tokio::test]
async fn a_public_request_is_executed_by_rust() {
    let (_dir, schema) = contract_schema();
    let document = serde_json::json!({
        "query": "mutation { createNote(input: {title: \"From the LAN\", content: \"\"}) { title } }",
    })
    .to_string();

    let body = run(&schema, &document, None).await.unwrap();
    let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        result["data"]["createNote"]["title"], "From the LAN",
        "{result}"
    );

    let read = serde_json::json!({
        "query": "query { notes(query: \"\", offset: 0, limit: 10) { title } }",
    })
    .to_string();
    let body = run(&schema, &read, None).await.unwrap();
    let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        result["data"]["notes"].as_array().map(Vec::len),
        Some(1),
        "the write and the read share one store, so both came from Rust: {result}"
    );
}

#[tokio::test]
async fn variables_and_operation_names_reach_the_resolver() {
    let (_dir, schema) = contract_schema();
    let document = serde_json::json!({
        "query": "query Note($title: String!) { noteCount(query: $title) }",
        "operationName": "Note",
        "variables": { "title": "anything" },
    })
    .to_string();

    let body = run(&schema, &document, None).await.unwrap();
    let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(result["data"]["noteCount"], 0, "{result}");
    assert!(result.get("errors").is_none(), "{result}");
}

/// A body that is not JSON is not an envelope at all, so it never reaches
/// the executor. An envelope with no document *is* one, and gets the
/// ordinary GraphQL parse error — the same answer the platform gave.
#[tokio::test]
async fn a_body_that_is_not_a_graphql_request_is_refused() {
    let (_dir, schema) = contract_schema();
    assert_eq!(
        run(&schema, "not json", None).await.unwrap_err(),
        StatusCode::BAD_REQUEST
    );

    let body = run(&schema, "{}", None).await.unwrap();
    let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        !result["errors"].as_array().is_none_or(Vec::is_empty),
        "{result}"
    );
}

/// A field the contract does not have is a schema error, not a crash and not
/// a silent empty object — the web console can tell a typo from a real miss.
#[tokio::test]
async fn an_unknown_field_is_reported_as_a_graphql_error() {
    let (_dir, schema) = contract_schema();
    let document = serde_json::json!({ "query": "query { noSuchRoot }" }).to_string();
    let body = run(&schema, &document, None).await.unwrap();
    let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        !result["errors"].as_array().is_none_or(Vec::is_empty),
        "{result}"
    );
}

/// The NAS can be driven before setup with a configured bearer. It must
/// stay narrow: exact token only, nothing when unset, and plaintext both
/// ways because there is no session key to encrypt with.
#[tokio::test]
async fn the_development_bearer_only_admits_the_exact_configured_token() {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();

    // Unset: no bearer of any kind opens the development path.
    assert!(
        plain_rs::prefs::system::dev_token(&core.shared_preferences().unwrap())
            .unwrap()
            .is_empty()
    );
    assert!(!dev_bearer_matches(&core.runtime_state(), "Bearer anything").unwrap());

    let prefs = core.shared_preferences().unwrap();
    prefs
        .set(plain_rs::prefs::system::keys::DEV_TOKEN, "nas-dev")
        .unwrap();
    let state = core.runtime_state();
    assert!(dev_bearer_matches(&state, "Bearer nas-dev").unwrap());
    assert!(!dev_bearer_matches(&state, "Bearer wrong").unwrap());
    assert!(!dev_bearer_matches(&state, "Bearer ").unwrap());
    // A prefix of the real token is not the token, and the scheme is required.
    assert!(!dev_bearer_matches(&state, "Bearer nas-de").unwrap());
    assert!(!dev_bearer_matches(&state, "nas-dev").unwrap());

    // Clearing it takes the entry point away again.
    prefs
        .remove(plain_rs::prefs::system::keys::DEV_TOKEN)
        .unwrap();
    assert!(!dev_bearer_matches(&state, "Bearer nas-dev").unwrap());
    core.stop_core().await;
}
