use super::*;
#[test]
fn receipts_include_only_physically_removed_nodes_without_following_symlinks() {
    let temp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    fs::create_dir_all(root.join("nested")).unwrap();
    fs::write(root.join("nested/file"), b"synthetic").unwrap();
    fs::write(outside.path().join("keep"), b"outside").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), root.join("link")).unwrap();
    let plan = Plan::inspect(&root).unwrap();
    assert_eq!(plan.files.len(), 1);
    let outcome = plan.execute();
    assert!(outcome.removed);
    assert!(outcome.failures.is_empty());
    assert!(!root.exists());
    assert!(outside.path().join("keep").is_file());
    assert!(!Plan::inspect(&root).unwrap().execute().removed);
    assert!(Plan::inspect(Path::new("/")).is_err());
    assert!(Plan::inspect(&temp.path().join(".")).is_err());
}
#[test]
fn changed_nodes_remain_failed_and_do_not_erase_receipts_for_other_successes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    fs::create_dir(&root).unwrap();
    let blocked = root.join("blocked");
    fs::write(&blocked, b"old").unwrap();
    let removed = root.join("removed");
    fs::write(&removed, b"synthetic").unwrap();
    let plan = Plan::inspect(&root).unwrap();
    fs::remove_file(&blocked).unwrap();
    fs::create_dir(&blocked).unwrap();
    fs::write(blocked.join("keep"), b"new").unwrap();
    let outcome = plan.execute();
    assert!(!outcome.removed);
    assert!(!outcome.failures.is_empty());
    assert!(
        outcome
            .paths
            .contains(&removed.to_string_lossy().into_owned())
    );
    assert!(
        !outcome
            .paths
            .contains(&blocked.to_string_lossy().into_owned())
    );
    assert!(blocked.join("keep").is_file());
    assert!(!removed.exists());
}

#[test]
fn plan_does_not_remove_replaced_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("original");
    std::fs::write(&path, "old").unwrap();
    let plan = super::Plan::inspect(&path).unwrap();
    std::fs::rename(&path, dir.path().join("old")).unwrap();
    std::fs::write(&path, "replacement").unwrap();
    let result = plan.execute();
    assert!(!result.removed);
    assert!(result.paths.is_empty());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "replacement");
    assert_eq!(result.failures.len(), 1);
}
