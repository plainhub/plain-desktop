use super::*;

#[test]
fn explicit_sources_accept_ordinary_directories_and_exclude_unrequested_volumes() {
    let directory = tempfile::tempdir().unwrap();
    let documents = directory.path().join("documents");
    let volume = directory.path().join("volume");
    std::fs::create_dir(&documents).unwrap();
    std::fs::create_dir(&volume).unwrap();
    let roots =
        choose_directories(&[], &[documents.clone()], Some(directory.path()), &[volume]).unwrap();
    assert_eq!(roots, [documents.canonicalize().unwrap().to_string_lossy()]);
}

#[test]
fn saved_sources_are_preserved_instead_of_replaced_by_usb_mounts() {
    let directory = tempfile::tempdir().unwrap();
    let saved = directory.path().join("saved");
    std::fs::create_dir(&saved).unwrap();
    let roots = choose_directories(
        &[saved.to_string_lossy().into_owned()],
        &[],
        Some(directory.path()),
        &[],
    )
    .unwrap();
    assert_eq!(roots, [saved.canonicalize().unwrap().to_string_lossy()]);
}

#[test]
fn default_sources_include_home_and_removable_volumes_without_nas_path_rules() {
    let directory = tempfile::tempdir().unwrap();
    let home = directory.path().join("home");
    let external = directory.path().join("external");
    std::fs::create_dir(&home).unwrap();
    std::fs::create_dir(&external).unwrap();
    let roots = choose_directories(&[], &[], Some(&home), &[external.clone()]).unwrap();
    assert_eq!(roots.len(), 2);
    for path in [home, external] {
        assert!(roots.contains(&path.canonicalize().unwrap().to_string_lossy().into_owned()));
    }
}

#[test]
fn nested_sources_are_scanned_once_and_missing_explicit_sources_fail() {
    let directory = tempfile::tempdir().unwrap();
    let nested = directory.path().join("photos");
    std::fs::create_dir(&nested).unwrap();
    let roots = choose_directories(
        &[],
        &[
            nested,
            directory.path().to_path_buf(),
            directory.path().to_path_buf(),
        ],
        None,
        &[],
    )
    .unwrap();
    assert_eq!(
        roots,
        [directory.path().canonicalize().unwrap().to_string_lossy()]
    );
    assert!(choose_directories(&[], &[directory.path().join("missing")], None, &[]).is_err());
}

#[test]
fn ordinary_startup_never_requires_mount_privileges() {
    validate_automount(false).unwrap();
    #[cfg(not(target_os = "linux"))]
    assert!(validate_automount(true).is_err());
}

#[tokio::test]
async fn startup_scan_indexes_configured_directories_through_the_headless_host() {
    use crate::services::files::media_scans::Status;
    use std::sync::Arc;
    let home = crate::platform::system::consts::home_directory().unwrap();
    let directory = tempfile::Builder::new()
        .prefix("scan-check-")
        .tempdir_in(home)
        .unwrap();
    let source = directory.path().join("documents");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("document.txt"), b"document").unwrap();
    let core = Arc::new(
        crate::assembly::server::ContentServer::open(&directory.path().join("core")).unwrap(),
    );
    let catalog = core.native_media_catalog().unwrap();
    catalog
        .set_source_directories(&[source.to_string_lossy().into_owned()])
        .unwrap();
    catalog
        .configure_scan_directories(&directory.path().join("cache"), None)
        .unwrap();
    let runtime = Arc::new(crate::host::headless::runtime::Runtime::new(
        core.clone(),
        vec![],
    ));
    let runner = tokio::spawn(runtime.run());
    let started = start_source_scan(&catalog).unwrap();
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let snapshot = catalog.file_scan_status().unwrap().unwrap();
            if matches!(
                snapshot.status,
                Status::Completed | Status::Failed | Status::Cancelled
            ) {
                break snapshot;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    let count = catalog.count(plain_rs::enums::DataType::Doc).unwrap();
    core.stop_core().await;
    runner.await.unwrap().unwrap();
    let completed = result.unwrap();
    assert_eq!(completed.id, started.id);
    assert_eq!(completed.status, Status::Completed, "{:?}", completed.error);
    assert_eq!(completed.processed_files, 1);
    assert_eq!(count, 1);
}
