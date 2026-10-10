use super::*;

#[tokio::test]
async fn checked_listing_preserves_order_pagination_and_missing_directory_errors() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("z-dir")).unwrap();
    std::fs::write(temp.path().join("a.txt"), b"synthetic").unwrap();
    std::fs::write(temp.path().join(".hidden"), b"synthetic").unwrap();
    let entries = list_dir_paged(temp.path(), false, 0, 10, SortBy::NameAsc)
        .await
        .unwrap();
    assert!(entries[0].is_dir);
    assert!(entries[1].path.ends_with("a.txt"));
    assert_eq!(entries.len(), 2);
    assert_eq!(
        list_dir_paged(temp.path(), false, 1, 1, SortBy::NameAsc)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        list_dir_paged(&temp.path().join("absent"), false, 0, 10, SortBy::NameAsc)
            .await
            .is_err()
    );
    assert!(
        list_dir(temp.path().join("a.txt").as_path(), true)
            .await
            .is_err()
    );
}
#[tokio::test]
async fn real_copy_and_move_preserve_existing_destination_and_return_actual_path() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("synthetic.bin");
    let destination = temp.path().join("out");
    std::fs::write(&source, b"source").unwrap();
    std::fs::create_dir(&destination).unwrap();
    std::fs::write(destination.join("synthetic.bin"), b"existing").unwrap();
    let copy = copy_path(&source, &destination, false).await.unwrap();
    assert_eq!(copy, destination.join("synthetic_1.bin"));
    assert_eq!(std::fs::read(&copy).unwrap(), b"source");
    assert_eq!(
        std::fs::read(destination.join("synthetic.bin")).unwrap(),
        b"existing"
    );
    let moved = move_path(&source, &destination, false).await.unwrap();
    assert_eq!(moved, destination.join("synthetic_2.bin"));
    assert_eq!(std::fs::read(&moved).unwrap(), b"source");
    assert!(!source.exists());
    let free = temp.path().join("free.bin");
    std::fs::write(&free, b"free").unwrap();
    assert_eq!(
        move_path(&free, &destination, false).await.unwrap(),
        destination.join("free.bin")
    );
}
#[tokio::test]
async fn cross_device_pipeline_streams_a_complete_file_and_removes_source() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let destination = temp.path().join("destination");
    let mut file = std::fs::File::create(&source).unwrap();
    std::io::Write::write_all(&mut file, &vec![42; 2 * 1024 * 1024]).unwrap();
    drop(file);
    copy_and_remove(&source, &destination, false).await.unwrap();
    assert!(!source.exists());
    assert_eq!(
        std::fs::read(&destination).unwrap(),
        vec![42; 2 * 1024 * 1024]
    );
}
#[tokio::test]
async fn copy_based_move_checkpoints_after_copy_and_before_source_removal() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let source_dir = tempfile::tempdir().unwrap();
    let source = source_dir.path().join("source");
    let destination = source_dir.path().join("destination");
    std::fs::write(&source, b"cross-volume fixture").unwrap();
    let observed = Arc::new(AtomicBool::new(false));
    let checkpoint_observed = observed.clone();
    let resolved = copy_remove_with_checkpoint(
        &source,
        &destination,
        false,
        None,
        move |source, target| async move {
            assert!(source.exists());
            assert_eq!(std::fs::read(&target).unwrap(), b"cross-volume fixture");
            checkpoint_observed.store(true, Ordering::SeqCst);
            Ok(())
        },
    )
    .await
    .unwrap();
    assert!(observed.load(Ordering::SeqCst));
    assert_eq!(resolved, destination);
    assert!(!source.exists());
    assert_eq!(std::fs::read(resolved).unwrap(), b"cross-volume fixture");
}
#[tokio::test]
async fn failed_copy_keeps_source_and_existing_destination() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let destination = temp.path().join("destination");
    std::fs::write(&source, b"source").unwrap();
    std::fs::write(&destination, b"existing").unwrap();
    assert!(copy_and_remove(&source, &destination, false).await.is_err());
    assert_eq!(std::fs::read(source).unwrap(), b"source");
    assert_eq!(std::fs::read(destination).unwrap(), b"existing");
}
#[cfg(unix)]
#[tokio::test]
async fn source_hardlink_or_symlink_directory_alias_cannot_be_overwritten() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let alias = temp.path().join("alias");
    std::fs::write(&source, b"source").unwrap();
    std::fs::hard_link(&source, &alias).unwrap();
    assert!(copy_path(&source, &alias, true).await.is_err());
    assert_eq!(std::fs::read(&source).unwrap(), b"source");
    let directory = temp.path().join("directory");
    std::fs::create_dir(&directory).unwrap();
    let link = temp.path().join("link");
    std::os::unix::fs::symlink(&directory, &link).unwrap();
    assert!(
        copy_path(&directory, &link.join("nested"), false)
            .await
            .is_err()
    );
    assert!(!directory.join("nested").exists());
    let copied = copy_path(&directory, &temp.path().join("new/empty"), false)
        .await
        .unwrap();
    assert!(copied.is_dir());
}
#[cfg(unix)]
#[tokio::test]
async fn atomic_no_replace_does_not_clobber_an_existing_file_or_directory() {
    let temp = tempfile::tempdir().unwrap();
    for directory in [false, true] {
        let source = temp.path().join(if directory {
            "source-dir"
        } else {
            "source-file"
        });
        let destination = temp
            .path()
            .join(if directory { "dest-dir" } else { "dest-file" });
        if directory {
            std::fs::create_dir(&source).unwrap();
            std::fs::create_dir(&destination).unwrap();
        } else {
            std::fs::write(&source, b"source").unwrap();
            std::fs::write(&destination, b"existing").unwrap();
        }
        assert_eq!(
            rename::no_replace(&source, &destination)
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::AlreadyExists
        );
        assert!(source.exists());
        if !directory {
            assert_eq!(std::fs::read(&destination).unwrap(), b"existing");
        }
    }
}

#[cfg(unix)]
#[test]
fn unique_names_preserve_dangling_links_and_surface_non_directory_ancestors() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("dangling");
    std::os::unix::fs::symlink(temp.path().join("absent"), &target).unwrap();
    assert_eq!(
        crate::utils::unique_path::unique_sibling(&target).unwrap(),
        temp.path().join("dangling_1")
    );
    let blocking = temp.path().join("blocking");
    std::fs::write(&blocking, b"synthetic").unwrap();
    assert!(crate::utils::unique_path::unique_sibling(&blocking.join("child")).is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn copying_a_dangling_symlink_preserves_the_link_itself() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source-link");
    let missing = temp.path().join("missing-target");
    std::os::unix::fs::symlink(&missing, &source).unwrap();
    let destination = copy_path(&source, &temp.path().join("destination-link"), false)
        .await
        .unwrap();
    assert_eq!(std::fs::read_link(destination).unwrap(), missing);
    assert!(
        std::fs::symlink_metadata(source)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[tokio::test]
async fn rename_preserves_existing_targets_and_only_changes_a_leaf_name() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let destination = temp.path().join("destination");
    std::fs::write(&source, b"source").unwrap();
    std::fs::write(&destination, b"existing").unwrap();
    assert!(rename(&source, &destination).await.is_err());
    assert_eq!(std::fs::read(&source).unwrap(), b"source");
    assert_eq!(std::fs::read(&destination).unwrap(), b"existing");
    let renamed = temp.path().join("renamed");
    rename(&source, &renamed).await.unwrap();
    assert!(!source.exists());
    assert_eq!(std::fs::read(renamed).unwrap(), b"source");
}
#[cfg(feature = "media")]
#[tokio::test]
async fn public_rename_rejects_traversal_and_keeps_the_original_file() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    std::fs::write(&source, b"source").unwrap();
    for name in ["../outside", "/outside", "", ".", ".."] {
        assert!(
            crate::media::file_ops::rename_file(source.to_str().unwrap(), name)
                .await
                .is_err()
        );
        assert_eq!(std::fs::read(&source).unwrap(), b"source");
    }
}
