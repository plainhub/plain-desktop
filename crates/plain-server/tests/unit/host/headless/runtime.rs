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
    assert_eq!(facts["deviceType"], "COMPUTER");
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
    assert_eq!(data["app"]["deviceType"], "COMPUTER");
    assert_eq!(
        data["app"]["capabilities"],
        serde_json::json!(["MEDIA_SCAN"])
    );
    assert_eq!(data["app"]["httpPort"], 0);
    assert_eq!(data["app"]["httpsPort"], 0);
    core.stop_core().await;
    runner.await.unwrap().unwrap();
}

#[tokio::test]
async fn native_file_resolution_uses_actual_os_read_authorization() {
    let directory = Directory::new();
    let path = directory.0.join("readable.txt");
    std::fs::write(&path, "actual bytes").unwrap();
    let core = Arc::new(ContentServer::open(&directory.0).unwrap());
    let runtime = Arc::new(Runtime::new(core.clone(), vec![]));
    assert!(
        runtime
            .dispatch("fileTaskAuthorize", serde_json::json!({"paths":[path]}))
            .await
            .is_err()
    );
    assert!(
        runtime
            .dispatch(
                "fileTaskAuthorize",
                serde_json::json!({"operation":"write","paths":[path]})
            )
            .await
            .is_err()
    );
    let runner = tokio::spawn(runtime.run());
    assert_eq!(
        core.resolve_native_file(path.to_str().unwrap())
            .await
            .unwrap(),
        path
    );
    let missing = directory.0.join("missing.txt");
    assert!(
        core.resolve_native_file(missing.to_str().unwrap())
            .await
            .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"actual bytes");
    core.stop_core().await;
    runner.await.unwrap().unwrap();
    assert!(
        core.resolve_native_file(path.to_str().unwrap())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn bucket_dispatch_uses_the_same_core_catalog_as_rows_and_count() {
    let directory = Directory::new();
    let core = Arc::new(ContentServer::open(&directory.0).unwrap());
    let path = directory.0.join("report.txt");
    core.native_media_catalog()
        .unwrap()
        .record_files(&[plain_rs::db::MediaFileMetadata {
            data_type: plain_rs::enums::DataType::Doc,
            path: path.to_str().unwrap().into(),
            title: "Document title".into(),
            artist: String::new(),
            bucket_id: directory.0.to_str().unwrap().into(),
            size_bytes: 12,
            duration_ms: 0,
            album_file_id: String::new(),
            width: 0,
            height: 0,
            rotation: 0,
            taken_at: None,
            created_at: "2026-10-13 00:00:00".into(),
            updated_at: "2026-10-13 00:00:00".into(),
        }])
        .unwrap();
    let runtime = Runtime::new(core.clone(), vec![]);
    let buckets = runtime
        .dispatch(
            "systemMediaBucketItemFacts",
            serde_json::json!({"dataType":"DOC"}),
        )
        .await
        .unwrap();
    let count = runtime
        .dispatch(
            "systemMediaCount",
            serde_json::json!({"dataType":"DOC","query":""}),
        )
        .await
        .unwrap();
    let rows = runtime.dispatch("systemMediaRows",serde_json::json!({"dataType":"DOC","query":"","sortBy":"NAME_ASC","limit":10,"offset":0})).await.unwrap();
    assert_eq!(count, serde_json::json!(1));
    assert_eq!(buckets.as_array().unwrap().len(), 1);
    assert_eq!(buckets[0]["id"], rows[0]["bucketId"]);
    assert_eq!(buckets[0]["path"], rows[0]["path"]);
    assert_eq!(buckets[0]["size"], rows[0]["size"]);
    assert!(
        runtime
            .dispatch(
                "systemMediaBucketItemFacts",
                serde_json::json!({"dataType":"DOC","query":"trash:true"})
            )
            .await
            .is_err()
    );
    let runtime = Arc::new(runtime);
    let runner = tokio::spawn(runtime.clone().run());
    let response = core
        .native_graphql()
        .unwrap()
        .execute("query { mediaBuckets(type: DOC) { id name itemCount topItemPaths } }".into())
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    assert_eq!(data["mediaBuckets"][0]["id"], directory.0.to_str().unwrap());
    assert_eq!(data["mediaBuckets"][0]["itemCount"], 1);
    assert_eq!(
        data["mediaBuckets"][0]["topItemPaths"],
        serde_json::json!([path])
    );
    core.stop_core().await;
    runner.await.unwrap().unwrap();
    assert!(
        runtime
            .dispatch(
                "systemMediaBucketItemFacts",
                serde_json::json!({"dataType":"DOC"})
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn completion_callback_dispatch_never_waits_for_another_host_request() {
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    let directory = Directory(home.join(plain_rs::utils::short_uuid::short_uuid()));
    std::fs::create_dir(&directory.0).unwrap();
    let core = Arc::new(ContentServer::open(&directory.0.as_path().join("core")).unwrap());
    core.native_media_catalog()
        .unwrap()
        .configure_scan_directories(&directory.0.as_path().join("cache"), None)
        .unwrap();
    let path = directory.0.as_path().join("completed.txt");
    std::fs::write(&path, b"completed file").unwrap();
    let runtime = Arc::new(Runtime::new(core.clone(), vec![]));
    let mut calls = tokio::task::JoinSet::new();
    for _ in 0..32 {
        let runtime = runtime.clone();
        let path = path.clone();
        calls.spawn(async move {
            runtime
                .dispatch("scanFilesFacts", serde_json::json!({"paths":[path]}))
                .await
        });
    }
    while let Some(result) = calls.join_next().await {
        assert_eq!(result.unwrap().unwrap(), serde_json::json!(true));
    }
    assert_eq!(
        core.native_media_catalog()
            .unwrap()
            .count(plain_rs::enums::DataType::Doc)
            .unwrap(),
        1
    );
    let missing = directory.0.as_path().join("missing.txt");
    assert!(
        runtime
            .dispatch("scanFilesFacts", serde_json::json!({"paths":[missing]}))
            .await
            .is_err()
    );
    core.stop_core().await;
}

#[tokio::test]
async fn device_and_log_queries_use_the_headless_server_provider_without_a_legacy_context() {
    let directory = Directory::new();
    let logs = directory.0.join("logs");
    std::fs::create_dir_all(&logs).unwrap();
    std::fs::write(logs.join("latest.log"), "first\nsecond\n").unwrap();
    let core = Arc::new(ContentServer::open(&directory.0).unwrap());
    let runtime = Arc::new(Runtime::new(core.clone(), Vec::new()));
    let runner = tokio::spawn(runtime.run());
    let response = core.native_graphql().unwrap().execute("{ deviceInfo { platform cpuArch appVersion } deviceStatus { uptimeSec cpuUsage } appLogs(query:\"\",offset:0,limit:2) }".into()).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    assert_eq!(data["deviceInfo"]["cpuArch"], std::env::consts::ARCH);
    assert_eq!(data["appLogs"], serde_json::json!(["second", "first"]));
    let response = core
        .native_graphql()
        .unwrap()
        .execute("mutation { clearAppLogs }".into())
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        std::fs::read(logs.join("latest.log")).unwrap(),
        Vec::<u8>::new()
    );
    core.stop_core().await;
    runner.await.unwrap().unwrap();
}

#[tokio::test]
async fn thumbnails_and_regular_file_probes_use_the_shared_media_engine() {
    let directory = Directory::new();
    let image_path = directory.0.join("photo.png");
    image::RgbImage::new(8, 4).save(&image_path).unwrap();
    let core = Arc::new(ContentServer::open(&directory.0).unwrap());
    core.native_media_catalog()
        .unwrap()
        .configure_scan_directories(&directory.0.join("cache"), None)
        .unwrap();
    let runtime = Arc::new(Runtime::new(core.clone(), Vec::new()));
    let runner = tokio::spawn(runtime.clone().run());
    let state = core.headless_state();
    let bytes = state
        .thumbnails
        .get(
            state,
            crate::services::images::thumbnails::Request {
                path: image_path.to_string_lossy().into_owned(),
                width: 4,
                height: 4,
                center_crop: true,
                media_id: String::new(),
                file_name: "photo.png".into(),
                if_none_match: None,
            },
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(image::load_from_memory(&bytes).unwrap().width(), 4);
    assert_eq!(image::load_from_memory(&bytes).unwrap().height(), 4);
    let value = runtime
        .dispatch(
            "systemFileResource",
            serde_json::json!({"operation":"animated","path":image_path,"params":{}}),
        )
        .await
        .unwrap();
    assert_eq!(value, Value::Bool(false));
    core.stop_core().await;
    runner.await.unwrap().unwrap();
}

#[tokio::test]
async fn recent_files_and_image_info_reuse_shared_storage_and_metadata() {
    let directory = Directory::new();
    let path = directory.0.join("photo.png");
    image::RgbImage::new(8, 4).save(&path).unwrap();
    let core = Arc::new(ContentServer::open(&directory.0).unwrap());
    let runtime = Runtime::new(core.clone(), Vec::new());
    let prefs = core.preferences().unwrap();
    plain_media::storage::kv::recent::add_recent_file(prefs, path.to_str().unwrap());
    plain_media::storage::kv::recent::add_recent_file(
        prefs,
        directory.0.join("missing").to_str().unwrap(),
    );
    let rows = runtime
        .dispatch("systemRecentFileFacts", serde_json::json!({}))
        .await
        .unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["path"], path.to_str().unwrap());
    assert_eq!(rows[0]["name"], "photo.png");
    let info = runtime
        .dispatch("systemImageFileInfo", serde_json::json!({"path":path}))
        .await
        .unwrap();
    assert_eq!(info["width"], 8);
    assert_eq!(info["height"], 4);
    assert!(
        runtime
            .dispatch(
                "systemImageFileInfo",
                serde_json::json!({"path":directory.0.join("missing.png")})
            )
            .await
            .unwrap()
            .is_null()
    );
    core.stop_core().await;
}
