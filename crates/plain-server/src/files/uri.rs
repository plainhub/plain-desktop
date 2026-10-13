use std::path::{Path, PathBuf};

use plain_rs::chat::attachments::app_file_store;

/// Decrypted `/fs` `id` payload.
#[derive(Debug, PartialEq, Eq)]
pub struct DecryptedId {
    /// A `fid:` URI, an `app://` URI, a relative path, or an absolute
    /// filesystem path.
    pub path: String,
    /// Optional display name; empty when the id carries none.
    pub name: String,
}

#[derive(serde::Deserialize)]
struct JsonId {
    #[serde(default)]
    path: String,
    #[serde(default)]
    name: String,
}

/// Parse the plaintext payload of an `/fs` `id` param: either a JSON
/// object `{"path":"…","name":"…"}` or a bare path/URI string.
pub fn parse_decrypted_id(plaintext: &str) -> DecryptedId {
    if plaintext.starts_with('{') {
        if let Ok(JsonId { path, name }) = serde_json::from_str(plaintext) {
            return DecryptedId { path, name };
        }
    }
    DecryptedId {
        path: plaintext.to_string(),
        name: String::new(),
    }
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
#[path = "../../tests/unit/files/uri.rs"]
mod tests;
