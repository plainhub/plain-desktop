use super::*;

#[test]
fn zip_virtual_paths_split_into_archive_and_prefix() {
    let dir = std::env::temp_dir().join(format!("plain_zip_path_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let archive = dir.join("pack.zip");
    std::fs::write(&archive, b"PK").unwrap();

    let (file, inner) = split_zip_path(&format!("{}!zip!/sub/deep", archive.display())).unwrap();
    assert_eq!(file, archive);
    assert_eq!(inner, "sub/deep");

    // A bare archive path is a plain file, exactly like `isZipPath` in the
    // Kotlin helper — only the explicit marker makes it a virtual path.
    assert!(split_zip_path(&archive.display().to_string()).is_none());
    let (file, inner) = split_zip_path(&format!("{}!zip!/", archive.display())).unwrap();
    assert_eq!(file, archive);
    assert_eq!(inner, "", "the archive root is an empty prefix");

    // A path that is not an existing archive is not a virtual path at all.
    assert!(split_zip_path(&format!("{}!zip!/x", dir.join("missing.zip").display())).is_none());
    assert!(split_zip_path(&dir.display().to_string()).is_none());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn download_names_are_percent_encoded() {
    assert_eq!(url_escape("plain.zip"), "plain.zip");
    assert_eq!(url_escape("报告 2024.zip"), "%E6%8A%A5%E5%91%8A%202024.zip");
    assert_eq!(url_escape("a\"b.zip"), "a%22b.zip");
}

#[test]
fn directory_walks_are_prefixed_and_recursive() {
    let dir = std::env::temp_dir().join(format!("plain_zip_walk_{}", std::process::id()));
    std::fs::create_dir_all(dir.join("nested")).unwrap();
    std::fs::write(dir.join("top.txt"), b"a").unwrap();
    std::fs::write(dir.join("nested/inner.txt"), b"b").unwrap();
    let mut items = Vec::new();
    walk_directory(&dir, "bundle", &mut items).unwrap();
    let mut names: Vec<String> = items.iter().map(|(_, name)| name.clone()).collect();
    names.sort();
    assert_eq!(names, vec!["bundle/nested/inner.txt", "bundle/top.txt"]);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn zip_files_requests_default_every_field() {
    let request: ZipFilesRequest = serde_json::from_str("{}").unwrap();
    assert!(request.r#type.is_empty());
    assert!(request.query.is_empty());
    assert!(request.id.is_empty());
    assert!(request.name.is_empty());
}
