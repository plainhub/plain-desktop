use super::*;
use crate::assembly::server::ContentServer;
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn local_and_remote_execute_the_same_envelope_and_preserve_errors() {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(plain_rs::prefs::Prefs::load(&dir.path().join("prefs.json")).unwrap());
    let token = plain_rs::utils::base64::base64_encode(&[8; 32]);
    let server = ContentServer::start(&dir.path().join("db"), &token, prefs).unwrap();
    let state = server.runtime_state();
    let document = json!({"query":"query Count($q: String!) { noteCount(query: $q) } query Other { __typename }", "operationName":"Count", "variables":{"q":""}}).to_string();
    let local = server
        .graphql_executor()
        .execute(serde_json::from_str(&document).unwrap(), false)
        .await;
    let remote = execute_graphql(
        &state.public,
        serde_json::from_str(&document).unwrap(),
        RequestContext::remote(Some(state.clone())),
    )
    .await;
    assert_eq!(
        serde_json::to_value(&local).unwrap(),
        serde_json::to_value(&remote).unwrap()
    );
    assert!(local.errors.is_empty());
    for host_api in [false, true] {
        let response = server
            .graphql_executor()
            .execute(Request::new("{ nonexistentField }"), host_api)
            .await;
        assert!(!response.errors.is_empty());
        assert!(serde_json::to_value(response).unwrap()["errors"].is_array());
    }
    let response = server
        .graphql_executor()
        .execute(Request::new("{ audioHostQueueItems { path } }"), false)
        .await;
    assert!(
        !response.errors.is_empty(),
        "host fields must not enter the public schema"
    );
    server.stop_public().await;
    let response = server
        .graphql_executor()
        .execute(Request::new(r#"{ noteCount(query: "") }"#), false)
        .await;
    assert!(
        response.errors.is_empty(),
        "native execution must survive HTTP shutdown"
    );
    server.shutdown().await;
}
