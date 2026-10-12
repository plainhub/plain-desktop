use super::*;
use async_graphql::Request;

#[tokio::test]
async fn native_private_media_controls_share_core_and_public_requires_storage_grants() {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let native = core.native_graphql().unwrap();
    let set = native
        .execute_private(Request::new(
            r#"mutation { setMediaSourceDirs(dirs:["/media/a"]) }"#,
        ))
        .await;
    assert!(set.errors.is_empty(), "{:?}", set.errors);
    let query = native
        .execute_private(Request::new(
            "{ mediaSourceDirs scanProgress { indexed pending total state error } }",
        ))
        .await;
    assert!(query.errors.is_empty(), "{:?}", query.errors);
    let data = query.data.into_json().unwrap();
    assert_eq!(data["mediaSourceDirs"], serde_json::json!(["/media/a"]));
    assert_eq!(data["scanProgress"]["state"], "IDLE");
    assert_eq!(
        core.native_media_catalog()
            .unwrap()
            .source_directories()
            .unwrap(),
        vec!["/media/a"]
    );
    let failed = native
        .execute_private(Request::new(
            r#"mutation { startMediaScan(root:"/media/a") }"#,
        ))
        .await;
    assert!(!failed.errors.is_empty());
    let state = core.runtime_state();
    let remote = crate::graphql::executor::execute_graphql(
        &state.schema,
        Request::new("{ mediaSourceDirs }"),
        crate::graphql::executor::RequestContext::remote(Some(state.clone())),
    )
    .await;
    assert!(!remote.errors.is_empty());
    let main = native
        .execute(Request::new("{ scanProgress { state } }"))
        .await;
    assert!(!main.errors.is_empty());
    core.stop_core().await;
}

#[tokio::test]
async fn public_scans_require_opt_in_and_grants_then_read_the_same_core() {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let state = core.runtime_state();
    let request = || Request::new("{ mediaSourceDirs scanProgress { indexed total state } }");
    let denied = crate::graphql::executor::execute_graphql(
        &state.public,
        request(),
        crate::graphql::executor::RequestContext::remote(Some(state.clone())),
    )
    .await;
    assert!(!denied.errors.is_empty());
    state
        .prefs
        .set(
            plain_rs::prefs::system::keys::API_PERMISSIONS,
            vec![super::super::public::STORAGE],
        )
        .unwrap();
    core.native_media_catalog()
        .unwrap()
        .set_source_directories(&["/media/a".to_string()])
        .unwrap();
    let host = state.host.clone();
    let (generation, mut requests) = host.connect();
    let grants = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let provider_grants = grants.clone();
    let (done, mut finish) = tokio::sync::oneshot::channel::<()>();
    let provider = tokio::spawn(async move {
        loop {
            let request = tokio::select! {
                _ = &mut finish => break,
                request = requests.recv() => request.unwrap(),
            };
            let granted = provider_grants.load(std::sync::atomic::Ordering::SeqCst);
            assert_eq!(request["method"], "systemPermissionFacts");
            assert_eq!(
                request["params"]["permissions"],
                serde_json::json!(["WRITE_EXTERNAL_STORAGE"])
            );
            host.reply(generation, serde_json::json!({"id":request["id"], "result":{"granted":{"WRITE_EXTERNAL_STORAGE":granted}}})).unwrap();
        }
    });
    let allowed = crate::graphql::executor::execute_graphql(
        &state.public,
        request(),
        crate::graphql::executor::RequestContext::remote(Some(state.clone())),
    )
    .await;
    assert!(allowed.errors.is_empty(), "{:?}", allowed.errors);
    let data = allowed.data.into_json().unwrap();
    assert_eq!(data["mediaSourceDirs"], serde_json::json!(["/media/a"]));
    assert_eq!(data["scanProgress"]["state"], "IDLE");
    grants.store(false, std::sync::atomic::Ordering::SeqCst);
    let denied = crate::graphql::executor::execute_graphql(
        &state.public,
        request(),
        crate::graphql::executor::RequestContext::remote(Some(state.clone())),
    )
    .await;
    assert!(!denied.errors.is_empty());
    done.send(()).unwrap();
    provider.await.unwrap();
    core.stop_core().await;
}
