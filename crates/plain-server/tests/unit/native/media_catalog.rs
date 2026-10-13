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

#[cfg(feature = "media")]
#[tokio::test]
async fn completion_scan_uses_the_object_protocol_and_os_reads_without_host_reentry() {
    let directory = directory_scan_fixture();
    let core =
        crate::assembly::server::ContentServer::open(&directory.path().join("core")).unwrap();
    let catalog = core.native_media_catalog().unwrap();
    catalog
        .configure_scan_directories(&directory.path().join("cache"), None)
        .unwrap();
    let path = directory.path().join("completed.txt");
    std::fs::write(&path, b"completed file").unwrap();
    assert!(!core.runtime_state().host.connected());
    assert!(
        catalog
            .scan_completed_files(serde_json::json!({"paths":[path]}))
            .unwrap()
    );
    assert_eq!(catalog.count(plain_rs::enums::DataType::Doc).unwrap(), 1);
    assert!(
        catalog
            .scan_completed_files(serde_json::json!([path]))
            .is_err()
    );
    assert!(
        catalog
            .scan_completed_files(serde_json::json!({"paths":[path],"extra":true}))
            .is_err()
    );
    assert!(
        catalog
            .scan_completed_files(serde_json::json!({"paths":[]}))
            .unwrap()
    );
    let opaque = directory.path().join("file.bin");
    std::fs::write(&opaque, b"opaque").unwrap();
    assert!(
        catalog
            .scan_completed_files(serde_json::json!({"paths":[opaque]}))
            .unwrap()
    );
    assert_eq!(catalog.count(plain_rs::enums::DataType::Doc).unwrap(), 1);
    core.stop_core().await;
    assert!(
        catalog
            .scan_completed_files(serde_json::json!({"paths":[]}))
            .is_err()
    );
}

#[cfg(feature = "media")]
#[tokio::test]
async fn completion_scan_reports_read_failures_instead_of_partial_success() {
    let directory = directory_scan_fixture();
    let core =
        crate::assembly::server::ContentServer::open(&directory.path().join("core")).unwrap();
    let catalog = core.native_media_catalog().unwrap();
    catalog
        .configure_scan_directories(&directory.path().join("cache"), None)
        .unwrap();
    let path = directory.path().join("completed.txt");
    std::fs::write(&path, b"completed file").unwrap();
    let missing = directory.path().join("missing.txt");
    assert!(
        catalog
            .scan_completed_files(serde_json::json!({"paths":[path,missing]}))
            .is_err()
    );
    assert_eq!(catalog.count(plain_rs::enums::DataType::Doc).unwrap(), 0);
    assert!(
        catalog
            .scan_completed_files(serde_json::json!({"paths":["relative.txt"]}))
            .is_err()
    );
    core.stop_core().await;
}

#[tokio::test]
async fn source_directories_share_core_preferences_and_reject_stopped_handles() {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let catalog = core.native_media_catalog().unwrap();
    let clone = catalog.clone();
    let sources = vec!["/media/a".to_string(), "/media/a".to_string()];
    catalog.set_source_directories(&sources).unwrap();
    assert_eq!(clone.source_directories().unwrap(), sources);
    assert_eq!(
        plain_rs::prefs::media::source_directories(&core.runtime_state().prefs).unwrap(),
        sources
    );
    core.stop_core().await;
    assert!(catalog.source_directories().is_err());
    assert!(catalog.set_source_directories(&[]).is_err());
}

#[cfg(feature = "media")]
#[tokio::test]
async fn source_directory_scan_keeps_one_session_across_roots_and_partial_failure() {
    use crate::services::files::media_scans::Status;
    let directory = directory_scan_fixture();
    let roots = [
        directory.path().join("first"),
        directory.path().join("second"),
    ];
    for root in &roots {
        std::fs::create_dir_all(root).unwrap();
        std::fs::write(root.join("document.txt"), b"document").unwrap();
    }
    let core =
        crate::assembly::server::ContentServer::open(&directory.path().join("core")).unwrap();
    let catalog = core.native_media_catalog().unwrap();
    catalog
        .configure_scan_directories(&directory.path().join("cache"), None)
        .unwrap();
    let sources = roots
        .iter()
        .map(|root| root.to_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    catalog.set_source_directories(&sources).unwrap();
    let host = core.runtime_state().host.clone();
    let (generation, mut requests) = host.connect();
    let observer = catalog.clone();
    let (done, mut finish) = tokio::sync::oneshot::channel::<()>();
    let authorize = tokio::spawn(async move {
        let mut id = None;
        loop {
            tokio::select! {
                _ = &mut finish => break,
                request = requests.recv() => {
                    let request = request.unwrap();
                    let snapshot = observer.file_scan_status().unwrap().unwrap();
                    assert_eq!(snapshot.status, Status::Running);
                    assert_eq!(snapshot.total_files, None);
                    if let Some(previous) = &id { assert_eq!(previous, &snapshot.id); }
                    else { id = Some(snapshot.id); }
                    host.reply(generation, serde_json::json!({"id":request["id"],"result":true})).unwrap();
                }
            }
        }
    });
    let result = catalog.scan_source_directories("/").await.unwrap();
    done.send(()).unwrap();
    authorize.await.unwrap();
    assert_eq!(result.status, Status::Completed);
    assert_eq!(result.total_files, Some(2));
    assert_eq!(result.processed_files, 2);
    assert_eq!(catalog.count(plain_rs::enums::DataType::Doc).unwrap(), 2);

    let host = core.runtime_state().host.clone();
    let (generation, mut requests) = host.connect();
    let authorize = tokio::spawn(async move {
        for _ in 0..3 {
            let request = requests.recv().await.unwrap();
            host.reply(
                generation,
                serde_json::json!({"id":request["id"],"result":true}),
            )
            .unwrap();
        }
    });
    let result = catalog
        .scan_directories(vec![
            sources[0].clone(),
            directory
                .path()
                .join("missing")
                .to_str()
                .unwrap()
                .to_owned(),
        ])
        .await
        .unwrap();
    authorize.await.unwrap();
    assert_eq!(result.status, Status::Failed);
    assert_eq!(result.processed_files, 1);
    assert_eq!(result.total_files, None);
    assert!(result.error.is_some());
    assert_ne!(
        result.id,
        catalog.scan_directories(Vec::new()).await.unwrap().id
    );
    core.stop_core().await;
}

#[cfg(feature = "media")]
#[tokio::test]
async fn background_source_scan_is_busy_before_authorization_and_core_stop_joins_it() {
    let directory = directory_scan_fixture();
    let root = directory.path().join("library");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("document.txt"), b"document").unwrap();
    let core =
        crate::assembly::server::ContentServer::open(&directory.path().join("core")).unwrap();
    let catalog = core.native_media_catalog().unwrap();
    catalog
        .configure_scan_directories(&directory.path().join("cache"), None)
        .unwrap();
    catalog
        .set_source_directories(&[root.to_str().unwrap().to_owned()])
        .unwrap();
    let host = core.runtime_state().host.clone();
    let (generation, mut requests) = host.connect();
    let started = catalog.start_source_scan("/").unwrap();
    assert!(catalog.start_source_scan("/").is_err());
    let request = requests.recv().await.unwrap();
    assert_eq!(request["method"], "fileTaskAuthorize");
    assert_eq!(catalog.file_scan_status().unwrap().unwrap().id, started.id);
    core.stop_core().await;
    assert_eq!(
        core.runtime_state()
            .media_scans
            .snapshot()
            .unwrap()
            .unwrap()
            .status,
        crate::services::files::media_scans::Status::Cancelled
    );
    let _ = host.reply(
        generation,
        serde_json::json!({"id":request["id"],"result":true}),
    );
    assert_eq!(
        crate::services::files::media_catalog::count(
            &core.runtime_state().db,
            plain_rs::enums::DataType::Doc,
            ""
        )
        .unwrap(),
        0
    );
}

#[tokio::test]
async fn action_id_requests_are_typed_and_stop_checked() {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let catalog = core.native_media_catalog().unwrap();
    assert!(
        catalog
            .ids_facts(serde_json::json!({"dataType":"DOC","query":"","fromTrash":false}))
            .unwrap()
            .is_empty()
    );
    for params in [
        serde_json::json!({"dataType":"DOC","query":""}),
        serde_json::json!({"dataType":"DOC","query":"","fromTrash":"false"}),
        serde_json::json!({"dataType":"DOC","query":"","fromTrash":false,"extra":true}),
    ] {
        assert!(catalog.ids_facts(params).is_err());
    }
    core.stop_core().await;
    assert!(
        catalog
            .ids_facts(serde_json::json!({"dataType":"DOC","query":"","fromTrash":false}))
            .is_err()
    );
}

/// The watcher writes into the catalog, so it must not outlive the core
/// that owns it: after `stop_core` no filesystem event can reach the store.
#[tokio::test]
async fn the_watcher_stops_with_the_core_and_rejects_after_it() {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(&directory.path()).unwrap();
    let source = directory.path().join("media");
    std::fs::create_dir_all(&source).unwrap();
    let catalog = core.native_media_catalog().unwrap();
    catalog
        .set_source_directories(&[source.to_string_lossy().into_owned()])
        .unwrap();
    // The scan policy is what decides what counts as media, so the watcher
    // cannot start before it is configured.
    assert!(
        catalog.watch_source_directories().is_err(),
        "watching before the scan policy exists has nothing to exclude against"
    );
    catalog
        .configure_scan_directories(&directory.path().join("cache"), None)
        .unwrap();
    assert!(catalog.watch_source_directories().unwrap());
    assert!(
        core.runtime_state().media_watch.lock().unwrap().is_some(),
        "a configured source directory must be watched"
    );
    // Re-watching replaces rather than stacking a second one.
    assert!(catalog.watch_source_directories().unwrap());

    core.stop_core().await;
    assert!(
        core.runtime_state().media_watch.lock().unwrap().is_none(),
        "the watcher has to be released with the core"
    );
    assert!(catalog.watch_source_directories().is_err());
    assert!(
        catalog
            .scan_completed_files(serde_json::json!({"paths":[]}))
            .is_err(),
        "a stopped core must refuse catalog writes"
    );
}

#[cfg(feature = "media")]
#[tokio::test]
async fn media_delete_returns_actual_os_receipts_and_keeps_failed_selection() {
    use plain_rs::media::actions::{Action, cleanup, validate};
    let directory = tempfile::tempdir().unwrap();
    let core =
        crate::assembly::server::ContentServer::open(&directory.path().join("core")).unwrap();
    let catalog = core.native_media_catalog().unwrap();
    let paths = [
        directory.path().join("one.txt"),
        directory.path().join("two.txt"),
    ];
    let metadata = paths
        .iter()
        .map(|path| {
            std::fs::write(path, b"document").unwrap();
            plain_rs::db::MediaFileMetadata {
                data_type: plain_rs::enums::DataType::Doc,
                path: path.to_str().unwrap().into(),
                title: "document".into(),
                artist: String::new(),
                bucket_id: directory.path().to_str().unwrap().into(),
                size_bytes: 8,
                duration_ms: 0,
                album_file_id: String::new(),
                width: 0,
                height: 0,
                rotation: 0,
                taken_at: None,
                created_at: "created".into(),
                updated_at: "updated".into(),
            }
        })
        .collect::<Vec<_>>();
    let rows = catalog.record_files(&metadata).unwrap();
    core.runtime_state()
        .db
        .with_conn(|conn| {
            conn.execute(
                "UPDATE media_files SET is_trashed=1 WHERE id=?1",
                [&rows[1].id],
            )
        })
        .unwrap();
    let ids = rows.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
    let outcome = catalog.delete_action_files(serde_json::json!({"type":24,"action":"DELETE","ids":ids,"fromTrash":false,"destDir":""})).unwrap();
    validate(
        plain_rs::enums::DataType::Doc,
        Action::Delete,
        &ids,
        &outcome,
    )
    .unwrap();
    assert_eq!(outcome.successful.len(), 1);
    assert_eq!(outcome.failed_ids, vec![rows[1].id.clone()]);
    assert!(!paths[0].exists());
    assert!(paths[1].exists());
    assert!(
        catalog
            .by_path(plain_rs::enums::DataType::Doc, paths[0].to_str().unwrap())
            .unwrap()
            .is_none()
    );
    cleanup(
        &core.runtime_state().db,
        plain_rs::enums::DataType::Doc,
        Action::Delete,
        &outcome.successful,
    )
    .unwrap();
    assert!(
        catalog
            .by_path(plain_rs::enums::DataType::Doc, paths[0].to_str().unwrap())
            .unwrap()
            .is_none()
    );
    assert!(
        catalog
            .by_path(plain_rs::enums::DataType::Doc, paths[1].to_str().unwrap())
            .unwrap()
            .is_some()
    );
    assert!(catalog.delete_action_files(serde_json::json!({"type":24,"action":"MOVE","ids":[],"fromTrash":false,"destDir":"/other"})).is_err());
    core.stop_core().await;
    assert!(catalog.delete_action_files(serde_json::json!({"type":24,"action":"DELETE","ids":[],"fromTrash":false,"destDir":""})).is_err());
}

#[cfg(feature = "media")]
#[tokio::test]
async fn image_delete_updates_index_before_any_parent_consumes_the_receipt() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("photo.jpg");
    std::fs::write(&path, b"image").unwrap();
    let core =
        crate::assembly::server::ContentServer::open(&directory.path().join("core")).unwrap();
    let catalog = core.native_media_catalog().unwrap();
    let rows = catalog
        .record_files(&[plain_rs::db::MediaFileMetadata {
            data_type: plain_rs::enums::DataType::Image,
            path: path.to_str().unwrap().into(),
            title: "photo".into(),
            artist: String::new(),
            bucket_id: directory.path().to_str().unwrap().into(),
            size_bytes: 5,
            duration_ms: 0,
            album_file_id: String::new(),
            width: 0,
            height: 0,
            rotation: 0,
            taken_at: None,
            created_at: "created".into(),
            updated_at: "updated".into(),
        }])
        .unwrap();
    let index = core.runtime_state().image_index.clone();
    let version = index.status().version;
    let outcome = catalog
        .delete_action_files(serde_json::json!({
            "type":plain_rs::enums::DataType::Image.kind(),"action":"DELETE",
            "ids":[rows[0].id],"fromTrash":false,"destDir":""
        }))
        .unwrap();
    assert!(outcome.failed_ids.is_empty());
    assert_eq!(outcome.successful.len(), 1);
    assert!(!path.exists());
    assert_eq!(catalog.count(plain_rs::enums::DataType::Image).unwrap(), 0);
    assert!(index.status().version > version);
    assert_eq!(index.status().indexed_images, 0);
    core.stop_core().await;
}

#[tokio::test]
async fn filesystem_watch_events_from_an_os_thread_update_the_shared_catalog() {
    let directory = tempfile::tempdir().unwrap();
    let source = tempfile::Builder::new()
        .prefix("watch-source-")
        .tempdir_in(std::env::current_dir().unwrap())
        .unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let catalog = core.native_media_catalog().unwrap();
    catalog
        .configure_scan_directories(&directory.path().join("cache"), None)
        .unwrap();
    catalog
        .set_source_directories(&[source.path().to_string_lossy().into_owned()])
        .unwrap();
    catalog.watch_source_directories().unwrap();
    let path = source.path().join("photo.png");
    image::RgbImage::new(1, 1).save(&path).unwrap();
    let path = path.to_string_lossy().into_owned();
    let observed = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            if catalog
                .by_path(plain_rs::enums::DataType::Image, &path)
                .unwrap()
                .is_some()
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    })
    .await;
    core.stop_core().await;
    assert!(
        observed.is_ok(),
        "OS watcher events must reach the owning Tokio runtime"
    );
}
