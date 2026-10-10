//! Filesystem-related helpers that don't deserve their own crate and that
//! we'd rather not pull in a third-party dependency for. Mirrors bits of
//! `internal/pkg/pathx` and the per-op resolvers in
//! `internal/graph/files_*_api.go` + `internal/graph/helpers_local.go` from
//! the Go side.
//!
//! Inlined here: `percent_decode_path` (used in HTTP query params).
//! Inlined elsewhere: per-file-id short id, mount table parsing, etc.

use std::path::Path;

/// Errors for file ID decryption.
#[derive(Debug)]
pub enum FileIdError {
    InvalidId,
    Forbidden,
}

/// Decrypt an encrypted file ID to a filesystem path.
/// Mirrors Go `fs.PathFromFileID` in `internal/fs/file_id.go`:
///   1. Base64-decode the id (replacing ' ' back to '+' for URL safety).
///   2. Load the URL token from the preferences.
///   3. Base64-decode the token to get the 32-byte XChaCha20-Poly1305 key.
///   4. Decrypt the ciphertext → plaintext file path.
pub fn path_from_file_id(id: &str, prefs: &crate::prefs::Prefs) -> Result<String, FileIdError> {
    let id = id.trim();
    if id.is_empty() {
        log::debug!("[path_from_file_id] empty id");
        return Err(FileIdError::InvalidId);
    }
    // Some URL parsers turn '+' into ' ' in query strings.
    let id = id.replace(' ', "+");
    log::debug!(
        "[path_from_file_id] id (first 40 chars): {}",
        &id[..id.len().min(40)]
    );

    let ciphertext = base64_decode(&id).ok_or_else(|| {
        log::debug!("[path_from_file_id] base64 decode of id failed");
        FileIdError::Forbidden
    })?;
    log::debug!("[path_from_file_id] ciphertext len: {}", ciphertext.len());

    let token = prefs
        .get::<String>("url_token")
        .ok()
        .flatten()
        .filter(|t| !t.is_empty())
        .ok_or_else(|| {
            log::debug!("[path_from_file_id] no url_token found in prefs");
            FileIdError::Forbidden
        })?;
    log::debug!(
        "[path_from_file_id] token: {}",
        &token[..token.len().min(20)]
    );

    let key = base64_decode(&token).ok_or_else(|| {
        log::debug!(
            "[path_from_file_id] base64 decode of token failed, token='{}'",
            token
        );
        FileIdError::Forbidden
    })?;
    log::debug!("[path_from_file_id] key len: {} (expected 32)", key.len());

    let plain = crate::crypto::xchacha_decrypt_raw(&key, &ciphertext).ok_or_else(|| {
        log::debug!(
            "[path_from_file_id] XChaCha20-Poly1305 decrypt failed, key_len={}, ct_len={}",
            key.len(),
            ciphertext.len()
        );
        FileIdError::Forbidden
    })?;
    if plain.is_empty() {
        log::debug!("[path_from_file_id] decrypted to empty");
        return Err(FileIdError::Forbidden);
    }
    let path = String::from_utf8(plain).map_err(|e| {
        log::debug!("[path_from_file_id] utf8 decode failed: {}", e);
        FileIdError::Forbidden
    })?;
    let path = extract_path(&path);
    log::debug!("[path_from_file_id] resolved to: {}", path);
    Ok(path)
}

/// plain-app FileServer contract: media-item ids encrypt a JSON envelope
/// `{"path":…,"mediaId":…}` (web `getFileId` mints it whenever the item has
/// a mediaId), while plain file ids encrypt the bare path. Accept both.
fn extract_path(plain: &str) -> String {
    let trimmed = plain.trim();
    if trimmed.starts_with('{') {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if let Some(p) = v.get("path").and_then(|p| p.as_str()) {
                return p.to_string();
            }
        }
    }
    plain.to_string()
}

/// Minimal base64 standard decoding (matches Go `base64.StdEncoding`).
fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let s = s.trim_end_matches('=');
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    for &b in s.as_bytes() {
        let val = match b {
            b'A'..=b'Z' => (b - b'A') as u32,
            b'a'..=b'z' => (b - b'a' + 26) as u32,
            b'0'..=b'9' => (b - b'0' + 52) as u32,
            b'+' => 62,
            b'/' => 63,
            b' ' => continue,
            _ => return None,
        };
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// Decode a percent-encoded path/URL component into a UTF-8 `String`.
///
/// Unlike `percent_encoding::percent_decode_str` we **only** support the
/// `%XX` syntax — no `+`-as-space handling, because we never feed query
/// strings into here (query strings are still parsed with the `url`
/// crate, which already decodes them). The intent is purely to undo the
/// `axum::extract::Path<String>` double-encoding for path segments that
/// contain `%` or non-ASCII characters.
///
/// On an invalid `%XX` triplet the bytes are passed through unchanged
/// (matching `decode_utf8_lossy` semantics on the upstream crate) so a
/// malformed URL doesn't bring the whole handler down.
pub fn percent_decode_path(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Map a file extension to an IANA MIME type. Falls back to
/// `application/octet-stream` for anything we don't recognise.
///
/// We don't try to be comprehensive — the Go side uses Go's `mime`
/// package which carries ~1000 entries, but for a NAS front-end the
/// ~50 entries below cover 99% of served files. Adding the rest is
/// trivial when the need arises.
pub fn guess_mime(path: &std::path::Path) -> String {
    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let mime = crate::utils::mime::mime_from_ext(filename);
    if mime.starts_with("text/") {
        format!("{mime}; charset=utf-8")
    } else {
        mime.to_string()
    }
}

/// Image extensions that qualify for the animated-image/SVG sniff below.
/// Mirrors plain-app `Constants.PHOTO_EXTENSIONS` (`isImageFast` gate).
const PHOTO_EXTENSIONS: [&str; 13] = [
    "jpg", "jpeg", "png", "bmp", "webp", "heic", "heif", "apng", "avif", "gif", "tiff", "tif",
    "svg",
];

/// Whether the file is an animated image (GIF, animated WebP, animated HEIF)
/// or an SVG. Decided from the extension plus a sniff of the first 256 content
/// bytes, mirroring plain-app `isAnimatedImageOrSvg` so the shared web client
/// sees identical `/fs` behaviour on both platforms: such files must be
/// served as-is (browsers render them natively, thumbnails included) instead
/// of being routed into the WebP thumbnail pipeline, which cannot decode them.
pub fn is_animated_image_or_svg(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    if !PHOTO_EXTENSIONS.contains(&ext.as_str()) {
        return false;
    }
    if ext == "svg" {
        return true;
    }
    if ext == "png" || ext == "jpg" || ext == "jpeg" {
        return false;
    }

    let mut header = [0u8; 256];
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let n = std::io::Read::read(&mut file, &mut header).unwrap_or(0);
    let header = &header[..n];

    // GIF87a / GIF89a magic at offset 0 — both served as-is (plain-app never
    // routes GIFs through the thumbnail pipeline on `/fs`).
    if n >= 6 && matches!(&header[..6], b"GIF87a" | b"GIF89a") {
        return true;
    }

    // JPEG / PNG magic bytes are never animated; stop the sniff cheaply.
    let b0 = header.first().copied();
    let b1 = header.get(1).copied();
    if b0 == Some(0xFF) && b1 == Some(0xD8) || b0 == Some(0x89) && b1 == Some(b'P') {
        return false;
    }

    // WebP: "RIFF" + "WEBP"; a "VP8X" box's animation bit is bit 1 of byte 16.
    if n >= 17 && &header[0..4] == b"RIFF" && &header[8..12] == b"WEBP" {
        return &header[12..16] == b"VP8X" && n > 17 && (header[16] & 0b10) != 0;
    }

    // HEIF: an "ftyp" box followed by an animated brand (msf1/hevc/hevx).
    if n >= 12 && &header[4..8] == b"ftyp" {
        return matches!(&header[8..12], b"msf1" | b"hevc" | b"hevx");
    }

    // SVG without a recognised extension: scan the readable window for the
    // `<svg` tag.
    header.windows(4).any(|w| w == b"<svg")
}

// ---------------------------------------------------------------------------
// File operations
//
// The functions below are 1:1 ports of the resolvers in
// `internal/graph/files_*_api.go` and the helpers in
// `internal/graph/helpers_local.go` + `helpers/files_helper.go` from the
// Go side. They are async because every caller in the GraphQL resolvers
// is `async fn` and we want the syscall work to run on tokio's blocking
// pool (via `tokio::fs::File`) rather than blocking the reactor.
// ---------------------------------------------------------------------------

pub use crate::filesystem::*;

#[cfg(test)]
use crate::filesystem::count_dir_entries_fast;

#[cfg(test)]
#[path = "../../tests/unit/media/fsx.rs"]
mod tests;
