use super::*;

#[test]
fn parse_decrypted_id_plain_path() {
    let (p, n) = parse_decrypted_id("fid:abc123def456.jpg");
    assert_eq!(p, "fid:abc123def456.jpg");
    assert!(n.is_empty());
}

#[test]
fn parse_decrypted_id_json_form() {
    let (p, n) = parse_decrypted_id(r#"{"path":"fid:abc.jpg","mediaId":"m1","name":"cat.jpg"}"#);
    assert_eq!(p, "fid:abc.jpg");
    assert_eq!(n, "cat.jpg");
}

#[test]
fn parse_decrypted_id_malformed_json_falls_back() {
    let (p, n) = parse_decrypted_id("{not json");
    assert_eq!(p, "{not json");
    assert!(n.is_empty());
}

#[test]
fn resolve_fid_with_ext() {
    let dir = std::path::Path::new("/data");
    let p = resolve_uri("fid:abcdef0123456789.jpg", dir);
    assert_eq!(
        p,
        std::path::PathBuf::from("/data/files/ab/cd/abcdef0123456789.jpg")
    );
}

#[test]
fn resolve_fid_without_ext() {
    let dir = std::path::Path::new("/data");
    let p = resolve_uri("fid:abcdef0123456789", dir);
    assert_eq!(
        p,
        std::path::PathBuf::from("/data/files/ab/cd/abcdef0123456789")
    );
}

#[test]
fn resolve_app_uri() {
    let dir = std::path::Path::new("/data");
    let p = resolve_uri("app://Pictures/test.png", dir);
    assert_eq!(p, std::path::PathBuf::from("/data/Pictures/test.png"));
}

#[test]
fn resolve_absolute_path() {
    let dir = std::path::Path::new("/data");
    let p = resolve_uri("/var/some/other/file.txt", dir);
    assert_eq!(p, std::path::PathBuf::from("/var/some/other/file.txt"));
}

#[test]
fn resolve_relative_path() {
    let dir = std::path::Path::new("/data");
    let p = resolve_uri("subdir/file.txt", dir);
    assert_eq!(p, std::path::PathBuf::from("/data/subdir/file.txt"));
}
