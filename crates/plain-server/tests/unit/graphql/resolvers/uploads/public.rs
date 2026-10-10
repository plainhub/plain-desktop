use super::*;

#[test]
fn names_keep_counters_flat_instead_of_nesting_them() {
    let dir = std::env::temp_dir().join(format!("plain_upload_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    assert_eq!(next_unique_name(&dir, "a.txt"), "a (1).txt");
    std::fs::write(dir.join("a.txt"), b"x").unwrap();
    std::fs::write(dir.join("a (1).txt"), b"x").unwrap();
    // plain-app strips the previous ` (N)` before appending, so the sequence is
    // a.txt -> a (1).txt -> a (2).txt, never `a (1) (1).txt`.
    assert_eq!(next_unique_name(&dir, "a.txt"), "a (2).txt");
    assert_eq!(next_unique_name(&dir, "noext"), "noext (1)");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn names_split_on_the_last_dot_only() {
    assert_eq!(split_name("photo.tar.gz"), ("photo.tar", "gz"));
    assert_eq!(split_name(".hidden"), (".hidden", ""));
    assert_eq!(split_name("plain"), ("plain", ""));
    assert_eq!(candidate("a", "", 2), "a (2)");
    assert_eq!(candidate("a", "txt", 2), "a (2).txt");
}

#[test]
fn upload_info_defaults_match_the_desktop_payload() {
    let info: UploadInfo = serde_json::from_str("{}").unwrap();
    assert!(info.dir.is_empty());
    assert!(!info.replace);
    assert!(!info.is_app_file);
    assert_eq!(info.size, 0);
    let chunk: ChunkInfo = serde_json::from_str("{}").unwrap();
    assert!(chunk.file_id.is_empty());
    assert_eq!(chunk.index, 0);
}
