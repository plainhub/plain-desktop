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

#[tokio::test]
async fn public_audit_events_read_the_core_database_without_host_or_legacy_store() {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let state = core.runtime_state();
    let id = state.db.audit_event_add("login", "ÄBC", "client").unwrap();
    let result = core.native_graphql().unwrap().execute(async_graphql::Request::new(
        r#"{ auditEvents(query:"text:äbc",offset:0,limit:10) { id type message clientId createdAt } }"#
    )).await;
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(result.data.into_json().unwrap()["auditEvents"][0]["id"], id);
    state
        .db
        .with_conn(|conn| conn.execute_batch("DROP TABLE audit_events"))
        .unwrap();
    let result = core
        .native_graphql()
        .unwrap()
        .execute(async_graphql::Request::new(
            r#"{ auditEvents(query:"",offset:0,limit:10) { id } }"#,
        ))
        .await;
    assert!(!result.errors.is_empty());
    core.stop_core().await;
}

#[tokio::test]
async fn public_disk_and_hostname_commands_reach_the_registered_provider() {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let state = core.runtime_state();
    let host = state.host.clone();
    let (generation, mut requests) = host.connect();
    let provider = tokio::spawn(async move {
        let request = requests.recv().await.unwrap();
        assert_eq!(request["method"], crate::host::platform::SYSTEM_DISK_FORMAT);
        assert_eq!(request["params"]["path"], "/dev/example");
        host.reply(
            generation,
            serde_json::json!({"id":request["id"],"error":"format refused"}),
        )
        .unwrap();
        let request = requests.recv().await.unwrap();
        assert_eq!(
            request["method"],
            crate::host::platform::SYSTEM_HOSTNAME_SET
        );
        assert_eq!(request["params"]["name"], "my-server");
        host.reply(
            generation,
            serde_json::json!({"id":request["id"],"result":true}),
        )
        .unwrap();
    });
    let executor = core.native_graphql().unwrap();
    let result = executor
        .execute("mutation { formatDisk(path:\"/dev/example\") }".into())
        .await;
    assert_eq!(result.errors[0].message, "format refused");
    let result = executor
        .execute("mutation { setHostname(name:\" My_Server \") }".into())
        .await;
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(result.data.into_json().unwrap()["setHostname"], true);
    provider.await.unwrap();
    let rows = state.db.audit_events(0, 10, None).unwrap();
    assert!(
        rows.iter()
            .any(|row| row.r#type == "set_hostname" && row.message == "my-server")
    );
    core.stop_core().await;
}

#[tokio::test]
async fn public_mount_alias_uses_the_same_preferences_as_the_platform_adapter() {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let result = core
        .native_graphql()
        .unwrap()
        .execute("mutation { setMountAlias(id:\"volume-example\",alias:\" Photos \") }".into())
        .await;
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(
        plain_rs::prefs::storage::get_map(core.preferences().unwrap())["volume-example"],
        "Photos"
    );
    core.stop_core().await;
}
