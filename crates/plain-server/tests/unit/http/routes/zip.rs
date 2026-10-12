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

fn write_archive(path: &Path, files: &[(&str, &[u8])]) {
    let mut writer = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, body) in files {
        writer.start_file(*name, options).unwrap();
        writer.write_all(body).unwrap();
    }
    writer.finish().unwrap();
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "plain_zip_test_{}_{}_{}",
        name,
        std::process::id(),
        NEXT_STAGING.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn every_request_stages_into_its_own_directory() {
    let first = Staging::create().unwrap();
    let second = Staging::create().unwrap();
    assert_ne!(
        first.directory, second.directory,
        "two requests must not share one staging directory"
    );

    std::fs::write(first.entry_path(0), b"first").unwrap();
    std::fs::write(second.entry_path(0), b"second").unwrap();

    assert_eq!(std::fs::read(first.entry_path(0)).unwrap(), b"first");
    assert_eq!(
        std::fs::read(second.entry_path(0)).unwrap(),
        b"second",
        "a concurrent zip must not overwrite another request's entry"
    );
}

#[test]
fn a_request_with_many_entries_keeps_them_apart() {
    let staging = Staging::create().unwrap();
    std::fs::write(staging.entry_path(0), b"one").unwrap();
    std::fs::write(staging.entry_path(1), b"two").unwrap();

    assert_ne!(staging.entry_path(0), staging.entry_path(1));
    assert_eq!(std::fs::read(staging.entry_path(1)).unwrap(), b"two");
}

#[test]
fn staged_entries_disappear_when_the_request_ends() {
    let staging = Staging::create().unwrap();
    std::fs::write(staging.entry_path(0), b"payload").unwrap();
    let directory = staging.directory.clone();
    assert!(directory.is_dir());

    drop(staging);

    assert!(
        !directory.exists(),
        "the staging directory outlived the request"
    );
}

#[test]
fn archive_entries_are_extracted_into_the_request_directory() {
    let dir = scratch("extract");
    let archive = dir.join("pack.zip");
    write_archive(&archive, &[("a.txt", b"alpha"), ("sub/b.txt", b"beta")]);
    let staging = Staging::create().unwrap();
    let mut items = Vec::new();

    collect_archive_entries(&archive, "", &staging, &mut items).unwrap();

    assert_eq!(items.len(), 2);
    let mut names: Vec<String> = items.iter().map(|(_, name)| name.clone()).collect();
    names.sort();
    assert_eq!(names, vec!["a.txt", "sub/b.txt"]);
    for (path, _) in &items {
        assert_eq!(
            path.parent(),
            Some(staging.directory.as_path()),
            "entries must live inside this request's staging directory"
        );
        assert!(path.is_file());
    }
    assert_eq!(std::fs::read(&items[0].0).unwrap(), b"alpha");
    assert_eq!(std::fs::read(&items[1].0).unwrap(), b"beta");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_unreadable_archive_leaves_no_staged_files_behind() {
    let dir = scratch("broken");
    let archive = dir.join("broken.zip");
    std::fs::write(&archive, b"not a zip at all").unwrap();
    let staging = Staging::create().unwrap();
    let directory = staging.directory.clone();
    let mut items = Vec::new();

    let failed = collect_archive_entries(&archive, "", &staging, &mut items).is_err();

    assert!(failed);
    assert!(items.is_empty());
    drop(staging);
    assert!(
        !directory.exists(),
        "a failed request must not leave staging files behind"
    );
    std::fs::remove_dir_all(&dir).ok();
}
