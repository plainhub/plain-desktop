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
    core.stop_core().await;
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
