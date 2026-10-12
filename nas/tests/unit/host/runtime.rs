use super::*;

struct Directory(std::path::PathBuf);

impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(plain_rs::utils::short_uuid::short_uuid());
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn app_facts_use_core_directory_and_unregistered_methods_fail() {
    let directory = Directory::new();
    let core = Arc::new(ContentServer::open(&directory.0).unwrap());
    let runtime = Runtime::new(core.clone(), vec!["MEDIA_SCAN".into()]);
    let facts = runtime
        .dispatch(SYSTEM_APP_FACTS, serde_json::json!({}))
        .await
        .unwrap();
    assert_eq!(
        facts["appDir"],
        directory.0.join("files").to_string_lossy().as_ref()
    );
    assert_eq!(facts["deviceType"], "NAS");
    assert_eq!(facts["capabilities"], serde_json::json!(["MEDIA_SCAN"]));
    assert!(
        runtime
            .dispatch("unregisteredMethod", serde_json::json!({}))
            .await
            .is_err()
    );
    core.stop_core().await;
}

#[tokio::test]
async fn native_app_query_uses_registered_facts_without_public_listener() {
    let directory = Directory::new();
    let core = Arc::new(ContentServer::open(&directory.0).unwrap());
    let runtime = Arc::new(Runtime::new(core.clone(), vec!["MEDIA_SCAN".into()]));
    let runner = tokio::spawn(runtime.run());
    let response = core
        .native_graphql()
        .unwrap()
        .execute("query { app { appDir deviceType capabilities httpPort httpsPort } }".into())
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    assert_eq!(
        data["app"]["appDir"],
        directory.0.join("files").to_string_lossy().as_ref()
    );
    assert_eq!(data["app"]["deviceType"], "NAS");
    assert_eq!(
        data["app"]["capabilities"],
        serde_json::json!(["MEDIA_SCAN"])
    );
    assert_eq!(data["app"]["httpPort"], 0);
    assert_eq!(data["app"]["httpsPort"], 0);
    core.stop_core().await;
    runner.await.unwrap().unwrap();
}
