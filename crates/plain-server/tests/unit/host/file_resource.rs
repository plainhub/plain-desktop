use super::*;
use serde_json::json;

#[test]
fn app_directory_uses_core_directory_and_mime_uses_shared_parser() {
    let directory = std::env::temp_dir();
    assert_eq!(
        facts(
            &directory,
            json!({"operation":"appDir", "path":"", "params":{}})
        )
        .unwrap(),
        directory.join("files").to_string_lossy().as_ref()
    );
    assert_eq!(
        facts(
            &directory,
            json!({"operation":"mime", "path":"photo.jpg", "params":{}})
        )
        .unwrap(),
        plain_rs::utils::mime::mime_from_ext("photo.jpg")
    );
}

#[test]
fn unsupported_and_malformed_operations_fail_without_fallback() {
    let directory = std::env::temp_dir();
    for request in [
        json!({"operation":"transcode", "path":"video.mp4", "params":{}}),
        json!({"operation":"mime", "path":"photo.jpg", "params":[]}),
        json!({"operation":"appDir", "path":"", "params":{}, "extra":true}),
    ] {
        assert!(facts(&directory, request).is_err());
    }
    assert!(
        facts(
            Path::new("relative"),
            json!({"operation":"appDir", "path":"", "params":{}})
        )
        .is_err()
    );
}

#[test]
fn read_authorization_checks_every_real_path_without_modifying_files() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("document.txt");
    std::fs::write(&path, b"original bytes").unwrap();
    assert!(authorize_read(json!({"operation":"read","paths":[path,directory.path()]})).unwrap());
    assert_eq!(std::fs::read(&path).unwrap(), b"original bytes");
    assert!(
        authorize_read(json!({"operation":"read","paths":[path,directory.path().join("missing")]}))
            .is_err()
    );
}

#[test]
fn read_authorization_never_grants_ambiguous_or_write_requests() {
    let directory = tempfile::tempdir().unwrap();
    for request in [
        json!({"paths":[directory.path()]}),
        json!({"type":"COPY","paths":[directory.path()]}),
        json!({"operation":"write","paths":[directory.path()]}),
        json!({"operation":"read","paths":[]}),
        json!({"operation":"read","paths":["relative.txt"]}),
        json!({"operation":"read","paths":[directory.path()],"extra":true}),
    ] {
        assert!(authorize_read(request).is_err());
    }
}

#[cfg(unix)]
#[test]
fn read_authorization_follows_existing_symlinks_and_rejects_broken_links() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.txt");
    let link = directory.path().join("link.txt");
    std::fs::write(&path, b"bytes").unwrap();
    std::os::unix::fs::symlink(&path, &link).unwrap();
    assert!(authorize_read(json!({"operation":"read","paths":[link]})).unwrap());
    std::fs::remove_file(&path).unwrap();
    assert!(authorize_read(json!({"operation":"read","paths":[link]})).is_err());
}
