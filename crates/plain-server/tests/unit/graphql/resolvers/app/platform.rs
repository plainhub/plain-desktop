use super::*;

#[tokio::test]
async fn public_samba_settings_and_mutation_use_core_preferences_and_explicit_host_methods() {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let state = core.runtime_state();
    let host = state.host.clone();
    let (generation, mut requests) = host.connect();
    let provider = tokio::spawn(async move {
        for method in [
            SYSTEM_SAMBA_SERVICE_FACTS,
            SYSTEM_SAMBA_SERVICE_FACTS,
            SYSTEM_SAMBA_APPLY,
        ] {
            let request = requests.recv().await.unwrap();
            assert_eq!(request["method"], method);
            let result = if method == SYSTEM_SAMBA_APPLY {
                assert_eq!(request["params"]["settings"]["enabled"], true);
                assert_eq!(request["params"]["settings"]["shares"][0]["auth"], "Guest");
                serde_json::Value::Bool(true)
            } else {
                serde_json::json!({"name":"smbd","active":true,"enabled":true})
            };
            host.reply(
                generation,
                serde_json::json!({"id":request["id"],"result":result}),
            )
            .unwrap();
        }
    });
    let query = core
        .native_graphql()
        .unwrap()
        .execute(async_graphql::Request::new(
            "{ sambaSettings { enabled serviceName serviceActive } }",
        ))
        .await;
    assert!(query.errors.is_empty(), "{:?}", query.errors);
    assert_eq!(
        query.data.into_json().unwrap()["sambaSettings"]["serviceActive"],
        true
    );
    let result = core.native_graphql().unwrap().execute(async_graphql::Request::new(
        r#"mutation { setSambaSettings(input:{enabled:true,shares:[{name:"media",sharePath:"/mnt/media",auth:GUEST,readOnly:true}]}) }"#
    )).await;
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert!(
        plain_rs::prefs::samba::get_samba_settings(&state.prefs)
            .unwrap()
            .enabled
    );
    provider.await.unwrap();
    core.stop_core().await;
}
