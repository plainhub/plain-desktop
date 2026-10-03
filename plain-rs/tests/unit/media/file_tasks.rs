use super::*;
#[tokio::test]
async fn queued_tasks_really_finish_and_kv_history_is_checked_and_owner_isolated() {
    let root = crate::media::paths::pin_test_data_dir();
    crate::media::kv::open(&root.join("fjall")).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let destination = temp.path().join("destination");
    std::fs::write(&source, b"synthetic").unwrap();
    let cid = format!("task-{}", crate::utils::shortid::new_id());
    let task = create_copy_task(
        &cid,
        vec![FileTaskOp {
            src: source.to_str().unwrap().into(),
            dst: destination.to_str().unwrap().into(),
            overwrite: false,
        }],
    )
    .unwrap();
    let completed = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let tasks = list_tasks(&cid).unwrap();
            assert_eq!(tasks.len(), 1);
            if matches!(
                tasks[0].status,
                FileTaskStatus::Done | FileTaskStatus::Error
            ) {
                break tasks[0].clone();
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(completed.status, FileTaskStatus::Done);
    assert_eq!(completed.done_bytes, 9);
    assert_eq!(completed.done_items, 1);
    assert_eq!(std::fs::read(destination).unwrap(), b"synthetic");
    let neighboring = format!("{cid}:neighbor");
    let foreign = FileTask {
        client_id: neighboring.clone(),
        id: "foreign".into(),
        ..completed.clone()
    };
    KvStore.put(&foreign).unwrap();
    assert_eq!(list_tasks(&cid).unwrap().len(), 1);
    assert_eq!(list_tasks(&neighboring).unwrap().len(), 1);
    let malformed = db_key(&cid, "broken");
    get_default().insert(&malformed, b"invalid JSON").unwrap();
    assert!(list_tasks(&cid).is_err());
    get_default().remove(malformed).unwrap();
    get_default().remove(db_key(&cid, &task.id)).unwrap();
    get_default()
        .remove(db_key(&neighboring, "foreign"))
        .unwrap();
}
