//! Unit tests for `src/chunked_upload.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;
use tempfile::TempDir;

/// Returns the path inside a fresh TempDir. The TempDir guard is held
/// by the caller so the directory isn't reaped mid-test.
async fn tmp_data_dir() -> (TempDir, PathBuf) {
    let guard = tempfile::tempdir().unwrap();
    let p = guard.path().to_path_buf();
    (guard, p)
}

#[tokio::test]
async fn save_then_list_chunks() {
    let (_guard, dir) = tmp_data_dir().await;
    save_chunk(&dir, "abc", 0, b"hello").await.unwrap();
    save_chunk(&dir, "abc", 1, b"world").await.unwrap();
    save_chunk(&dir, "abc", 2, b"!").await.unwrap();

    let list = list_uploaded_chunks(&dir, "abc").await.unwrap();
    assert_eq!(list, vec![0, 1, 2]);
}

#[tokio::test]
async fn list_unknown_file_id_is_empty() {
    let (_guard, dir) = tmp_data_dir().await;
    let list = list_uploaded_chunks(&dir, "nope").await.unwrap();
    assert!(list.is_empty());
}

#[tokio::test]
async fn merge_concatenates_in_order() {
    let (_guard, dir) = tmp_data_dir().await;
    save_chunk(&dir, "f1", 0, b"hello ").await.unwrap();
    save_chunk(&dir, "f1", 1, b"chunked ").await.unwrap();
    save_chunk(&dir, "f1", 2, b"world").await.unwrap();

    let dest = dir.join("merged.bin");
    let (name, size) = merge_chunks(&dir, "f1", 3, dest.to_str().unwrap(), true)
        .await
        .unwrap();
    assert_eq!(name, "merged.bin");
    assert_eq!(size, b"hello chunked world".len() as u64);
    let body = std::fs::read(&dest).unwrap();
    assert_eq!(body, b"hello chunked world");

    // Chunks must be gone
    let list = list_uploaded_chunks(&dir, "f1").await.unwrap();
    assert!(list.is_empty(), "chunks should be removed after merge");
}

#[tokio::test]
async fn merge_rejects_invalid_args() {
    let (_guard, dir) = tmp_data_dir().await;
    assert!(merge_chunks(&dir, "", 3, "/tmp/x", true).await.is_err());
    assert!(merge_chunks(&dir, "f", 0, "/tmp/x", true).await.is_err());
    assert!(merge_chunks(&dir, "f", 3, "", true).await.is_err());
}

#[tokio::test]
async fn merge_replace_false_appends_variant() {
    let (_guard, dir) = tmp_data_dir().await;
    // Pre-existing file at dest
    let dest = dir.join("myfile.txt");
    std::fs::write(&dest, b"existing").unwrap();
    // Pre-existing variant (1)
    let v1 = dir.join("myfile (1).txt");
    std::fs::write(&v1, b"existing-v1").unwrap();

    save_chunk(&dir, "f2", 0, b"fresh").await.unwrap();
    let (name, size) = merge_chunks(&dir, "f2", 1, dest.to_str().unwrap(), false)
        .await
        .unwrap();
    assert_eq!(name, "myfile (2).txt");
    assert_eq!(size, b"fresh".len() as u64);
    let body = std::fs::read(&v1).unwrap();
    assert_eq!(body, b"existing-v1", "variant (1) must be untouched");
    let new_body = std::fs::read(dir.join("myfile (2).txt")).unwrap();
    assert_eq!(new_body, b"fresh");
}

#[tokio::test]
async fn merge_replace_true_overwrites() {
    let (_guard, dir) = tmp_data_dir().await;
    let dest = dir.join("over.txt");
    std::fs::write(&dest, b"old").unwrap();

    save_chunk(&dir, "f3", 0, b"new").await.unwrap();
    let (name, size) = merge_chunks(&dir, "f3", 1, dest.to_str().unwrap(), true)
        .await
        .unwrap();
    assert_eq!(name, "over.txt");
    assert_eq!(size, b"new".len() as u64);
    let body = std::fs::read(&dest).unwrap();
    assert_eq!(body, b"new");
}

#[tokio::test]
async fn merge_missing_chunk_fails() {
    let (_guard, dir) = tmp_data_dir().await;
    let dest = dir.join("missing.bin");
    let res = merge_chunks(&dir, "f4", 2, dest.to_str().unwrap(), true).await;
    assert!(res.is_err());
}

#[tokio::test]
async fn save_chunk_rejects_bad_input() {
    let (_guard, dir) = tmp_data_dir().await;
    assert!(save_chunk(&dir, "", 0, b"x").await.is_err());
    assert!(save_chunk(&dir, "f", -1, b"x").await.is_err());
}
