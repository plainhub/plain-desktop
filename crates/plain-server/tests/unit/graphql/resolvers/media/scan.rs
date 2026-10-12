use super::*;
use async_graphql::Request;

#[tokio::test]
async fn native_private_media_controls_share_core_and_http_does_not_gain_them() {
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
