use super::*;

#[tokio::test]
async fn named_operation_reaches_shared_schema() {
    let state = crate::server::test_support::nas_state();
    let response = execute_graphql(
        &crate::http_server::main_schemas::build_schema(),
        json!({
            "query": "query Ignored { app { clientId } } query Selected { app { deviceType } }",
            "operationName": "Selected"
        }),
        state.ctx,
        "client".to_string(),
    )
    .await;
    assert_eq!(response["data"]["app"]["deviceType"], "NAS");
    assert!(response.get("errors").is_none());
}

#[tokio::test]
async fn client_id_reaches_mutation_resolvers() {
    let state = crate::server::test_support::nas_state();
    let cid = "executor-client".to_string();
    let response = execute_graphql(
        &crate::http_server::main_schemas::build_schema(),
        json!({ "query": "mutation { logout }" }),
        state.ctx.clone(),
        cid.clone(),
    )
    .await;
    assert!(response.get("errors").is_none(), "{response}");
    let events = crate::media::kv::EventLog::new(&state.ctx.media.db)
        .list(0, 10, None)
        .unwrap();
    assert!(events.iter().any(|event| event.client_id == cid));
}
