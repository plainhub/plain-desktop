#[tokio::test]
async fn catalog_is_created_from_core_and_stopped_handles_reject_access() {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let catalog = core.native_media_catalog().unwrap();
    assert_eq!(catalog.count(plain_rs::enums::DataType::Image).unwrap(), 0);
    assert!(catalog.record_files(&[]).unwrap().is_empty());
    assert!(
        catalog
            .list(plain_rs::enums::DataType::Image, 10, 0)
            .unwrap()
            .is_empty()
    );
    assert!(
        catalog
            .bucket_item_facts(serde_json::json!({"dataType":"IMAGE"}))
            .unwrap()
            .is_empty()
    );
    assert!(
        catalog
            .bucket_item_facts(serde_json::json!({"dataType":"CONTACT"}))
            .is_err()
    );
    assert!(
        catalog
            .bucket_item_facts(serde_json::json!({"dataType":"IMAGE","extra":true}))
            .is_err()
    );
    core.stop_core().await;
    assert!(
        catalog
            .bucket_item_facts(serde_json::json!({"dataType":"IMAGE"}))
            .is_err()
    );
    assert!(core.native_media_catalog().is_err());
    assert!(catalog.count(plain_rs::enums::DataType::Image).is_err());
    assert!(
        catalog
            .by_path(plain_rs::enums::DataType::Image, "/photo.jpg")
            .is_err()
    );
    assert!(
        catalog
            .list(plain_rs::enums::DataType::Image, 10, 0)
            .is_err()
    );
    assert!(catalog.record_files(&[]).is_err());
}

#[cfg(feature = "media")]
#[tokio::test]
async fn authorized_file_metadata_reaches_catalog_and_existing_facts_projection() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("report.txt");
    std::fs::write(&path, "document").unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let host = core.runtime_state().host.clone();
    let (generation, mut requests) = host.connect();
    let authorization = tokio::spawn(async move {
        for allow in [true, false] {
            let request = requests.recv().await.unwrap();
            assert_eq!(request["method"], "fileTaskAuthorize");
            assert_eq!(request["params"]["operation"], "read");
            host.reply(
                generation,
                serde_json::json!({"id":request["id"],"result":allow}),
            )
            .unwrap();
        }
    });
    let catalog = core.native_media_catalog().unwrap();
    let indexed = catalog
        .scan_file(plain_rs::enums::DataType::Doc, path.to_str().unwrap())
        .await
        .unwrap();
    assert_eq!(indexed.size_bytes, 8);
    assert_eq!(
        catalog
            .count_matching(plain_rs::enums::DataType::Doc, "ext:txt")
            .unwrap(),
        1
    );
    let rows = catalog.rows_facts(serde_json::json!({"dataType":"DOC","query":"ext:txt","sortBy":"NAME_ASC","limit":10,"offset":0})).unwrap();
    assert_eq!(rows[0]["id"], indexed.id);
    assert_eq!(rows[0]["size"], 8);
    assert!(!rows[0].as_object().unwrap().contains_key("originalPath"));
    assert!(
        catalog
            .scan_file(plain_rs::enums::DataType::Doc, path.to_str().unwrap())
            .await
            .is_err()
    );
    assert_eq!(catalog.count(plain_rs::enums::DataType::Doc).unwrap(), 1);
    authorization.await.unwrap();
    core.stop_core().await;
    assert!(
        catalog
            .scan_file(plain_rs::enums::DataType::Doc, path.to_str().unwrap())
            .await
            .is_err()
    );
}

#[cfg(feature = "media")]
#[tokio::test]
async fn rescan_with_failed_probe_retains_current_known_metadata_after_authorization() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("damaged.mp3");
    std::fs::write(&path, b"unrecognized audio bytes").unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let catalog = core.native_media_catalog().unwrap();
    let mut known = plain_media::metadata::file_facts::read_file_metadata(
        &path,
        plain_rs::enums::DataType::Audio,
    )
    .unwrap();
    known.title = "Known title".into();
    known.artist = "Known artist".into();
    known.duration_ms = 1500;
    known.album_file_id = "known-album".into();
    known.size_bytes = 999;
    let original = catalog.record_files(&[known]).unwrap().remove(0);
    let host = core.runtime_state().host.clone();
    let (generation, mut requests) = host.connect();
    let authorization = tokio::spawn(async move {
        let request = requests.recv().await.unwrap();
        assert_eq!(request["params"]["operation"], "read");
        host.reply(
            generation,
            serde_json::json!({"id":request["id"],"result":true}),
        )
        .unwrap();
    });
    let updated = catalog
        .scan_file(plain_rs::enums::DataType::Audio, path.to_str().unwrap())
        .await
        .unwrap();
    assert_eq!(updated.id, original.id);
    assert_eq!(updated.title, original.title);
    assert_eq!(updated.artist, original.artist);
    assert_eq!(updated.duration_ms, original.duration_ms);
    assert_eq!(updated.album_file_id, original.album_file_id);
    assert_eq!(
        updated.size_bytes,
        std::fs::metadata(path).unwrap().len() as i64
    );
    authorization.await.unwrap();
    core.stop_core().await;
    assert!(catalog.record_file_reads(&[]).is_err());
}

#[cfg(feature = "media")]
#[tokio::test]
async fn file_scan_cancel_aborts_pending_authorization_and_keeps_catalog_unchanged() {
    use crate::services::files::media_scans::Status;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("report.txt");
    std::fs::write(&path, "document").unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let catalog = core.native_media_catalog().unwrap();
    let host = core.runtime_state().host.clone();
    let (generation, mut requests) = host.connect();
    let worker = tokio::spawn({
        let catalog = catalog.clone();
        let path = path.clone();
        async move {
            catalog
                .scan_files(
                    plain_rs::enums::DataType::Doc,
                    vec![path.to_str().unwrap().into()],
                )
                .await
        }
    });
    let request = requests.recv().await.unwrap();
    assert_eq!(
        catalog.file_scan_status().unwrap().unwrap().status,
        Status::Running
    );
    assert!(
        catalog
            .scan_files(plain_rs::enums::DataType::Doc, vec![])
            .await
            .is_err()
    );
    assert_eq!(
        catalog.cancel_file_scan().unwrap().unwrap().status,
        Status::Cancelling
    );
    let result = worker.await.unwrap().unwrap();
    assert_eq!(result.status, Status::Cancelled);
    assert_eq!(result.processed_files, 0);
    host.reply(
        generation,
        serde_json::json!({"id":request["id"],"result":true}),
    )
    .unwrap();
    assert_eq!(catalog.count(plain_rs::enums::DataType::Doc).unwrap(), 0);
    assert_eq!(
        catalog
            .scan_files(plain_rs::enums::DataType::Doc, vec![])
            .await
            .unwrap()
            .status,
        Status::Completed
    );
    core.stop_core().await;
    assert!(catalog.file_scan_status().is_err());
    assert!(core.runtime_state().media_scans.begin(1).is_err());
}

#[cfg(feature = "media")]
#[tokio::test]
async fn aborted_scan_future_releases_the_shared_job_and_failed_scans_report_partial_progress() {
    use crate::services::files::media_scans::Status;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("report.txt");
    std::fs::write(&path, "document").unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let catalog = core.native_media_catalog().unwrap();
    let host = core.runtime_state().host.clone();
    let (generation, mut requests) = host.connect();
    let worker = tokio::spawn({
        let catalog = catalog.clone();
        let path = path.clone();
        async move {
            catalog
                .scan_files(
                    plain_rs::enums::DataType::Doc,
                    vec![path.to_str().unwrap().into()],
                )
                .await
        }
    });
    requests.recv().await.unwrap();
    worker.abort();
    assert!(worker.await.unwrap_err().is_cancelled());
    assert_eq!(
        catalog.file_scan_status().unwrap().unwrap().status,
        Status::Cancelled
    );
    let authorization = tokio::spawn(async move {
        let request = requests.recv().await.unwrap();
        host.reply(
            generation,
            serde_json::json!({"id":request["id"],"result":true}),
        )
        .unwrap();
    });
    let result = catalog
        .scan_files(
            plain_rs::enums::DataType::Doc,
            vec![path.to_str().unwrap().into(), "invalid-reference".into()],
        )
        .await
        .unwrap();
    assert_eq!(result.status, Status::Failed);
    assert_eq!(result.total_files, Some(2));
    assert_eq!(result.processed_files, 1);
    assert!(result.error.is_some());
    assert_eq!(catalog.count(plain_rs::enums::DataType::Doc).unwrap(), 1);
    authorization.await.unwrap();
    core.stop_core().await;
}

#[cfg(feature = "media")]
fn directory_scan_fixture() -> tempfile::TempDir {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    tempfile::Builder::new()
        .prefix("media-scan-")
        .tempdir_in(home)
        .unwrap()
}

#[cfg(feature = "media")]
#[tokio::test]
async fn directory_scan_uses_core_policy_real_walker_and_existing_catalog() {
    use crate::services::files::media_scans::Status;
    let directory = directory_scan_fixture();
    let root = directory.path().join("library");
    let cache = root.join("cache");
    let excluded = root.join("excluded");
    let data = root.join("core");
    for folder in [
        &root,
        &cache,
        &excluded,
        &root.join("nested"),
        &root.join(".hidden"),
        &root.join("marked"),
    ] {
        std::fs::create_dir_all(folder).unwrap();
    }
    for name in [
        "one.txt",
        "nested/two.txt",
        "photo.heic",
        "cache/cache.txt",
        "excluded/excluded.txt",
        ".hidden/hidden.txt",
        "marked/marked.txt",
    ] {
        std::fs::write(root.join(name), b"media facts").unwrap();
    }
    std::fs::write(root.join("empty.txt"), b"").unwrap();
    std::fs::write(root.join("marked/.nomedia"), b"").unwrap();
    let config = directory.path().join("config.toml");
    std::fs::write(
        &config,
        format!("[media_scan]\nexcluded_dirs = {}\n", excluded.display()),
    )
    .unwrap();
    let core = crate::assembly::server::ContentServer::open(&data).unwrap();
    std::fs::write(data.join("internal.txt"), b"internal").unwrap();
    let catalog = core.native_media_catalog().unwrap();
    assert!(
        catalog
            .scan_directory(root.to_str().unwrap())
            .await
            .is_err()
    );
    catalog
        .configure_scan_directories(&cache, Some(&config))
        .unwrap();
    let host = core.runtime_state().host.clone();
    let (generation, mut requests) = host.connect();
    let (done, mut finish) = tokio::sync::oneshot::channel::<()>();
    let authorize = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut finish => break,
                request=requests.recv() => {
                    let request=request.unwrap();
                    assert_eq!(request["method"],"fileTaskAuthorize");
                    assert_eq!(request["params"]["operation"],"read");
                    host.reply(generation,serde_json::json!({"id":request["id"],"result":true})).unwrap();
                },
            }
        }
    });
    let result = catalog
        .scan_directory(root.to_str().unwrap())
        .await
        .unwrap();
    done.send(()).unwrap();
    authorize.await.unwrap();
    assert_eq!(result.status, Status::Completed);
    assert_eq!(result.total_files, Some(3));
    assert_eq!(result.processed_files, 3);
    assert_eq!(catalog.count(plain_rs::enums::DataType::Doc).unwrap(), 2);
    assert_eq!(catalog.count(plain_rs::enums::DataType::Image).unwrap(), 1);
    assert!(
        catalog
            .by_path(
                plain_rs::enums::DataType::Doc,
                cache.join("cache.txt").to_str().unwrap()
            )
            .unwrap()
            .is_none()
    );
    core.stop_core().await;
}

#[cfg(feature = "media")]
#[tokio::test]
async fn directory_scan_cancels_during_root_authorization_without_discovery_or_writes() {
    use crate::services::files::media_scans::Status;
    let directory = tempfile::tempdir().unwrap();
    let core =
        crate::assembly::server::ContentServer::open(&directory.path().join("core")).unwrap();
    let catalog = core.native_media_catalog().unwrap();
    catalog
        .configure_scan_directories(&directory.path().join("cache"), None)
        .unwrap();
    let host = core.runtime_state().host.clone();
    let (_generation, mut requests) = host.connect();
    let worker = tokio::spawn({
        let catalog = catalog.clone();
        let root = directory.path().to_str().unwrap().to_owned();
        async move { catalog.scan_directory(&root).await }
    });
    requests.recv().await.unwrap();
    assert_eq!(
        catalog.file_scan_status().unwrap().unwrap().total_files,
        None
    );
    assert!(
        catalog
            .configure_scan_directories(&directory.path().join("other-cache"), None)
            .is_err()
    );
    catalog.cancel_file_scan().unwrap();
    let result = worker.await.unwrap().unwrap();
    assert_eq!(result.status, Status::Cancelled);
    assert_eq!(result.processed_files, 0);
    assert_eq!(catalog.count(plain_rs::enums::DataType::Doc).unwrap(), 0);
    core.stop_core().await;
}

#[cfg(feature = "media")]
#[tokio::test]
async fn directory_scan_cancellation_releases_a_bounded_producer_without_committing_pending_files()
{
    use crate::services::files::media_scans::Status;
    let directory = directory_scan_fixture();
    let root = directory.path().join("files");
    std::fs::create_dir(&root).unwrap();
    for index in 0..96 {
        std::fs::write(root.join(format!("{index}.txt")), b"data").unwrap();
    }
    let core =
        crate::assembly::server::ContentServer::open(&directory.path().join("core")).unwrap();
    let catalog = core.native_media_catalog().unwrap();
    catalog
        .configure_scan_directories(&directory.path().join("cache"), None)
        .unwrap();
    let host = core.runtime_state().host.clone();
    let (generation, mut requests) = host.connect();
    let worker = tokio::spawn({
        let catalog = catalog.clone();
        let root = root.to_str().unwrap().to_owned();
        async move { catalog.scan_directory(&root).await }
    });
    let request = requests.recv().await.unwrap();
    host.reply(
        generation,
        serde_json::json!({"id":request["id"],"result":true}),
    )
    .unwrap();
    let pending = requests.recv().await.unwrap();
    catalog.cancel_file_scan().unwrap();
    let result = worker.await.unwrap().unwrap();
    assert_eq!(result.status, Status::Cancelled);
    assert_eq!(result.processed_files, 0);
    assert_eq!(result.total_files, None);
    host.reply(
        generation,
        serde_json::json!({"id":pending["id"],"result":true}),
    )
    .unwrap();
    assert_eq!(catalog.count(plain_rs::enums::DataType::Doc).unwrap(), 0);
    core.stop_core().await;
}
