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

use serde_json::Value;

/// Convert `fid:` URIs to `fsid:` URIs for peer delivery. Mirrors
/// `DMessageContent.toPeerMessageContent()`: each item's `uri` is
/// encrypted with the local URL token and prefixed with `fsid:` so the
/// receiver can fetch it via the sender's `/fs` endpoint. The stored
/// content keeps the original `fid:` URIs.
pub fn to_peer_content(content: &str, token: &str) -> String {
    let Ok(mut v) = serde_json::from_str::<Value>(content) else {
        return content.to_string();
    };
    let Some(items) = v
        .get_mut("value")
        .and_then(|v| v.get_mut("items"))
        .and_then(|v| v.as_array_mut())
    else {
        return content.to_string();
    };
    for item in items.iter_mut() {
        if let Some(uri) = item.get("uri").and_then(|u| u.as_str())
            && uri.starts_with("fid:")
        {
            let encrypted = make_file_id(uri, token);
            if !encrypted.is_empty()
                && let Some(obj) = item.as_object_mut()
            {
                obj.insert(
                    "uri".to_string(),
                    Value::String(format!("fsid:{encrypted}")),
                );
            }
        }
    }
    v.to_string()
}

pub fn peer_content(content: &str, token: &str) -> anyhow::Result<String> {
    let mut value: Value = serde_json::from_str(content)?;
    if !matches!(
        value.get("type").and_then(Value::as_str),
        Some("FILES" | "IMAGES")
    ) {
        return Ok(content.into());
    }
    if crate::base64_decode(token).len() != 32 {
        anyhow::bail!("Invalid URL token");
    }
    let items = value
        .get_mut("value")
        .and_then(|v| v.get_mut("items"))
        .and_then(Value::as_array_mut)
        .ok_or_else(|| anyhow::anyhow!("Invalid attachment items"))?;
    for item in items {
        let uri = item
            .get("uri")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("Missing attachment URI"))?;
        let file_id = make_file_id(uri, token);
        if file_id.is_empty() {
            anyhow::bail!("Attachment encryption failed");
        }
        item.as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("Invalid attachment"))?
            .insert("uri".into(), Value::String(format!("fsid:{file_id}")));
    }
    Ok(value.to_string())
}
