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
