//! URL-token file-id encryption the `/fs` endpoint decrypts. Mirrors
//! plain-app `FileHelper.getFileId`.

use crate::base64_encode;
use crate::xchacha_encrypt;

/// Encrypt `path` with the local URL token and base64-encode the
/// result. Mirrors `plain-app`'s `FileHelper.getFileId(path)`.
pub fn make_file_id(path: &str, token: &str) -> String {
    let Some(encrypted) = xchacha_encrypt(token, path.as_bytes()) else {
        return String::new();
    };
    base64_encode(&encrypted)
}

#[cfg(test)]
#[path = "../../tests/unit/chat/content.rs"]
mod tests;
