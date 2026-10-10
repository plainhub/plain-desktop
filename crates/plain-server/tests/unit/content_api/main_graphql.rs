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
    crate::content_api::public_schema::PublicSchema,
) {
    use crate::{db::Db, prefs::Prefs};
    use std::sync::Arc;
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(Prefs::load(&dir.path().join("prefs.json")).unwrap());
    let db = Arc::new(Db::open(&dir.path().join("data.db")).unwrap());
    let (events, _) = tokio::sync::broadcast::channel(16);
    let directory = dir.path().to_path_buf();
    (
        dir,
        crate::content_api::public_schema::build(
            Arc::new(crate::content_api::host::Host::default()),
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
