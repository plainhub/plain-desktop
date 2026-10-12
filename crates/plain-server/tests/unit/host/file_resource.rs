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
