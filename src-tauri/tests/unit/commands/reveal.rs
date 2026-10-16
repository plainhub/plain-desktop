use super::*;

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "plain-file-actions-{}",
            plain_rs::utils::short_uuid::short_uuid()
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn platform_actions_accept_existing_absolute_files_only() {
    let root = Directory::new();
    let file = root.0.join("document.txt");
    std::fs::write(&file, b"document").unwrap();
    assert_eq!(
        existing_file(file.to_str().unwrap()).unwrap(),
        std::fs::canonicalize(&file).unwrap()
    );
    assert!(existing_file(root.0.to_str().unwrap()).is_err());
    assert!(existing_file(root.0.join("missing").to_str().unwrap()).is_err());
    for reference in [
        "fid:abcdef.txt",
        "app://document.txt",
        "document.txt",
        "file:///document.txt",
    ] {
        assert!(existing_file(reference).is_err(), "{reference}");
    }
}

#[test]
fn display_name_uses_the_selected_files_basename() {
    assert_eq!(display_name(Path::new("document.txt")), "document.txt");
    assert_eq!(display_name(Path::new("")), "file");
}

#[test]
fn saving_copies_the_selected_file_and_preserves_a_same_path_save() {
    let root = Directory::new();
    let source = root.0.join("source.txt");
    let destination = root.0.join("destination.txt");
    std::fs::write(&source, b"document").unwrap();
    let source = existing_file(source.to_str().unwrap()).unwrap();
    copy_to(&source, &destination).unwrap();
    assert_eq!(std::fs::read(&destination).unwrap(), b"document");
    copy_to(&source, &source).unwrap();
    assert_eq!(std::fs::read(&source).unwrap(), b"document");
}

#[cfg(unix)]
#[test]
fn saving_through_an_alias_of_the_source_keeps_the_source_bytes() {
    let root = Directory::new();
    let source = root.0.join("source.txt");
    let destination = root.0.join("alias.txt");
    std::fs::write(&source, b"document").unwrap();
    std::os::unix::fs::symlink(&source, &destination).unwrap();
    let source = existing_file(source.to_str().unwrap()).unwrap();
    copy_to(&source, &destination).unwrap();
    assert_eq!(std::fs::read(&source).unwrap(), b"document");
}
