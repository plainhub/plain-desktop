use super::*;

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "plain-file-actions-{}",
            plain_rs::libs::utils::short_uuid::short_uuid()
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
