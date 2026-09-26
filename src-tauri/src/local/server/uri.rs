use std::path::{Path, PathBuf};

use plain_rs::chat::app_file_store;

/// Parse the plaintext payload of an `/fs` `id` param. Returns
/// `(path, json_name)`. The path may be a `fid:` URI, an `app://`
/// URI, a relative path, or an absolute filesystem path.
pub(super) fn parse_decrypted_id(plaintext: &str) -> (String, String) {
    if plaintext.starts_with('{') {
        // JSON object form: {"path":"…","mediaId":"…","name":"…"}
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(plaintext) {
            let p = v
                .get("path")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let n = v
                .get("name")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            return (p, n);
        }
    }
    (plaintext.to_string(), String::new())
}

/// Resolve a virtual URI string to a real on-disk path under
/// `{data_dir}`. Mirrors the Kotlin `String.getFinalPath()` extension:
///
///   * `fid:{hash}.{ext}` → `{data_dir}/files/{aa}/{bb}/{hash}.{ext}`
///   * `fid:{hash}`       → `{data_dir}/files/{aa}/{bb}/{hash}`
///   * `app://{rel}`      → `{data_dir}/{rel}`
///   * absolute path      → returned as-is
///   * relative path      → joined to `{data_dir}`
pub(crate) fn resolve_uri(uri: &str, data_dir: &Path) -> PathBuf {
    if let Some(suffix) = uri.strip_prefix("fid:") {
        let (hash, ext) = match suffix.split_once('.') {
            Some((h, e)) => (h, e),
            None => (suffix, ""),
        };
        return app_file_store::dest_path(data_dir, hash, ext);
    }
    if let Some(suffix) = uri.strip_prefix("app://") {
        return data_dir.join(suffix);
    }
    let p = PathBuf::from(uri);
    if p.is_absolute() {
        p
    } else {
        data_dir.join(uri)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_decrypted_id_plain_path() {
        let (p, n) = parse_decrypted_id("fid:abc123def456.jpg");
        assert_eq!(p, "fid:abc123def456.jpg");
        assert!(n.is_empty());
    }

    #[test]
    fn parse_decrypted_id_json_form() {
        let (p, n) =
            parse_decrypted_id(r#"{"path":"fid:abc.jpg","mediaId":"m1","name":"cat.jpg"}"#);
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
}
