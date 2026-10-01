use std::path::{Path, PathBuf};

use crate::chat::app_file_store;

/// Parse the plaintext payload of an `/fs` `id` param. Returns
/// `(path, json_name)`. The path may be a `fid:` URI, an `app://`
/// URI, a relative path, or an absolute filesystem path.
pub fn parse_decrypted_id(plaintext: &str) -> (String, String) {
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
pub fn resolve_uri(uri: &str, data_dir: &Path) -> PathBuf {
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
#[path = "../../../tests/unit/api/server/uri.rs"]
mod tests;
