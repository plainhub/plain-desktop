use super::*;
#[tokio::test]
async fn out_of_order_chunks_merge_by_index_and_success_removes_chunks() {
    let temp = tempfile::tempdir().unwrap();
    let chunks = temp.path().join("chunks");
    tokio::fs::create_dir(&chunks).await.unwrap();
    tokio::fs::write(chunks.join("chunk_1"), b"def")
        .await
        .unwrap();
    tokio::fs::write(chunks.join("chunk_0"), b"abc")
        .await
        .unwrap();
    let target = temp.path().join("out/file.txt");
    let result = merge(
        None,
        &chunks,
        2,
        6,
        Kind::File {
            path: target.clone(),
            replace: false,
        },
    )
    .await
    .unwrap();
    assert_eq!(result, ("file.txt".into(), 6));
    assert_eq!(tokio::fs::read(target).await.unwrap(), b"abcdef");
    assert!(!chunks.exists());
}
#[tokio::test]
async fn missing_chunk_preserves_chunks_and_never_replaces_destination() {
    let temp = tempfile::tempdir().unwrap();
    let chunks = temp.path().join("chunks");
    tokio::fs::create_dir(&chunks).await.unwrap();
    tokio::fs::write(chunks.join("chunk_0"), b"abc")
        .await
        .unwrap();
    let target = temp.path().join("file");
    tokio::fs::write(&target, b"original").await.unwrap();
    assert!(
        merge(
            None,
            &chunks,
            2,
            6,
            Kind::File {
                path: target.clone(),
                replace: true
            }
        )
        .await
        .is_err()
    );
    assert!(chunks.exists());
    assert_eq!(tokio::fs::read(target).await.unwrap(), b"original");
}
#[tokio::test]
async fn size_mismatch_discards_stale_chunks_without_changing_target() {
    let temp = tempfile::tempdir().unwrap();
    let chunks = temp.path().join("chunks");
    tokio::fs::create_dir(&chunks).await.unwrap();
    tokio::fs::write(chunks.join("chunk_0"), b"abc")
        .await
        .unwrap();
    let target = temp.path().join("file");
    assert!(
        merge(
            None,
            &chunks,
            1,
            4,
            Kind::File {
                path: target.clone(),
                replace: true
            }
        )
        .await
        .is_err()
    );
    assert!(!chunks.exists());
    assert!(!target.exists());
}
#[test]
fn chunk_ids_cannot_escape_the_upload_directory() {
    for id in ["", ".", "..", "../a", "a/b", "a\\b", "a\0b"] {
        assert!(chunk_dir(Path::new("/tmp/uploads"), id).is_err());
    }
    assert_eq!(
        Runtime::default().status("unknown"),
        json!({"status":"NONE"})
    );
}
