//! HTTP upload endpoints for local mode chat file uploads.
//!
//! Mirrors `plain-app/.../web/routes/Upload.kt`:
//!
//! - `POST /upload`        — direct (small-file) upload, multipart with an
//!   `info` part (XChaCha20-encrypted JSON) and a `file` part.
//! - `POST /upload_chunk`  — single chunk of a larger file, same multipart
//!   shape but `info` carries `{fileId, index, size}`.
//!
//! Both endpoints authenticate via the `c-id` header. The `info` part is
//! encrypted with the local server's URL token (`ctx.token`); the server
//! decrypts it with the same token, matching how `Upload.kt` authenticates
//! against `HttpServerManager.tokenCache[clientId]`.
//!
//! ## Why a sync multipart parser on a fully-buffered body
//!
//! Chat uploads are bounded to a 5 MB chunk size (the web client's
//! `CHUNK_SIZE` in `lib/upload/upload.ts`). Buffering the full multipart
//! body in memory is therefore cheap — at most ~5 MB + small encrypted
//! info blob. This lets us use a simple, correct, sync parser instead of
//! writing (and stress-testing) a hand-rolled streaming async parser.

use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};

use crate::local::app_file_store;
use crate::local::graphql::context::AppCtx;
use super::response::respond;

const MAX_BODY_BYTES: usize = 16 * 1024 * 1024; // 16 MB safety cap; the client
                                                // should never send more
                                                // than ~5 MB per request.

pub(super) async fn handle_upload<R, W>(
    rd: R,
    mut wr: W,
    ctx: &Arc<AppCtx>,
    content_type: &str,
    content_length: usize,
) where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let boundary = match parse_multipart_boundary(content_type) {
        Some(b) => b,
        None => {
            respond(&mut wr, 400, "Bad Request", b"missing multipart boundary", "text/plain").await;
            return;
        }
    };
    if content_length > MAX_BODY_BYTES {
        respond(
            &mut wr,
            413,
            "Payload Too Large",
            format!("upload body exceeds {} bytes", MAX_BODY_BYTES).as_bytes(),
            "text/plain",
        )
        .await;
        return;
    }
    let body = match read_body_exact(rd, content_length).await {
        Ok(b) => b,
        Err(e) => {
            respond(&mut wr, 400, "Bad Request", e.as_bytes(), "text/plain").await;
            return;
        }
    };
    let parts = match parse_multipart(&body, &boundary) {
        Ok(p) => p,
        Err(e) => {
            respond(&mut wr, 400, "Bad Request", e.as_bytes(), "text/plain").await;
            return;
        }
    };

    let info_bytes = match parts.iter().find(|p| p.name == "info") {
        Some(p) => p.body,
        None => {
            respond(&mut wr, 400, "Bad Request", b"missing info part", "text/plain").await;
            return;
        }
    };
    let file_part = match parts.iter().find(|p| p.name == "file" && p.filename.is_some()) {
        Some(p) => p,
        None => {
            respond(&mut wr, 400, "Bad Request", b"missing file part", "text/plain").await;
            return;
        }
    };

    // Decrypt info JSON.
    let Some(plaintext) = plain_rs::xchacha_decrypt(&ctx.token, info_bytes) else {
        respond(&mut wr, 401, "Unauthorized", b"", "text/plain").await;
        return;
    };
    let info: serde_json::Value = match serde_json::from_slice(&plaintext) {
        Ok(v) => v,
        Err(e) => {
            respond(&mut wr, 400, "Bad Request", format!("bad info json: {e}").as_bytes(), "text/plain").await;
            return;
        }
    };
    let is_app_file = info.get("isAppFile").and_then(|v| v.as_bool()).unwrap_or(false);
    let info_dir = info.get("dir").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let info_replace = info.get("replace").and_then(|v| v.as_bool()).unwrap_or(false);
    let info_size = info.get("size").and_then(|v| v.as_i64()).unwrap_or(0);

    if info_size > 0 && file_part.body.len() as i64 != info_size {
        let msg = format!("Size mismatch: expected {info_size}, got {}", file_part.body.len());
        respond(&mut wr, 400, "Bad Request", msg.as_bytes(), "text/plain").await;
        return;
    }

    // Stage the file body to a temp file (in case `isAppFile` triggers the
    // hash + dedup pipeline, which needs a real on-disk file).
    let temp = match stage_to_temp(ctx, file_part.body).await {
        Ok(p) => p,
        Err(e) => {
            respond(&mut wr, 500, "Internal Server Error", e.as_bytes(), "text/plain").await;
            return;
        }
    };

    if is_app_file {
        let file_name = file_part.filename.clone().unwrap_or_default();
        match app_file_store::import_file(&ctx.db, &ctx.data_dir, &temp, &file_name, &file_part.content_type.clone().unwrap_or_default()) {
            Ok(result) => {
                let _ = tokio::fs::remove_file(&temp).await;
                respond(&mut wr, 201, "Created", result.fid_suffix.as_bytes(), "text/plain").await;
            }
            Err(e) => {
                let _ = tokio::fs::remove_file(&temp).await;
                respond(&mut wr, 500, "Internal Server Error", e.to_string().as_bytes(), "text/plain").await;
            }
        }
    } else {
        let safe_name = std::path::Path::new(file_part.filename.as_deref().unwrap_or("file"))
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("file")
            .to_string();
        let target = if info_dir.is_empty() {
            ctx.data_dir.join(&safe_name)
        } else {
            PathBuf::from(&info_dir).join(&safe_name)
        };
        if let Some(parent) = target.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        let _ = info_replace; // accepted but not yet wired into "auto-rename on conflict"
        match tokio::fs::copy(&temp, &target).await {
            Ok(_) => {
                let _ = tokio::fs::remove_file(&temp).await;
                respond(&mut wr, 201, "Created", safe_name.as_bytes(), "text/plain").await;
            }
            Err(e) => {
                let _ = tokio::fs::remove_file(&temp).await;
                respond(&mut wr, 500, "Internal Server Error", e.to_string().as_bytes(), "text/plain").await;
            }
        }
    }
}

pub(super) async fn handle_upload_chunk<R, W>(
    rd: R,
    mut wr: W,
    ctx: &Arc<AppCtx>,
    content_type: &str,
    content_length: usize,
) where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let boundary = match parse_multipart_boundary(content_type) {
        Some(b) => b,
        None => {
            respond(&mut wr, 400, "Bad Request", b"missing multipart boundary", "text/plain").await;
            return;
        }
    };
    if content_length > MAX_BODY_BYTES {
        respond(
            &mut wr,
            413,
            "Payload Too Large",
            format!("upload body exceeds {} bytes", MAX_BODY_BYTES).as_bytes(),
            "text/plain",
        )
        .await;
        return;
    }
    let body = match read_body_exact(rd, content_length).await {
        Ok(b) => b,
        Err(e) => {
            respond(&mut wr, 400, "Bad Request", e.as_bytes(), "text/plain").await;
            return;
        }
    };
    let parts = match parse_multipart(&body, &boundary) {
        Ok(p) => p,
        Err(e) => {
            respond(&mut wr, 400, "Bad Request", e.as_bytes(), "text/plain").await;
            return;
        }
    };

    let info_bytes = match parts.iter().find(|p| p.name == "info") {
        Some(p) => p.body,
        None => {
            respond(&mut wr, 400, "Bad Request", b"missing info part", "text/plain").await;
            return;
        }
    };
    let file_part = match parts.iter().find(|p| p.name == "file") {
        Some(p) => p,
        None => {
            respond(&mut wr, 400, "Bad Request", b"missing file part", "text/plain").await;
            return;
        }
    };

    let Some(plaintext) = plain_rs::xchacha_decrypt(&ctx.token, info_bytes) else {
        respond(&mut wr, 401, "Unauthorized", b"", "text/plain").await;
        return;
    };
    let info: serde_json::Value = match serde_json::from_slice(&plaintext) {
        Ok(v) => v,
        Err(e) => {
            respond(&mut wr, 400, "Bad Request", format!("bad info json: {e}").as_bytes(), "text/plain").await;
            return;
        }
    };
    let file_id = info.get("fileId").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let index = info.get("index").and_then(|v| v.as_i64()).unwrap_or(-1) as i32;
    let expected_size = info.get("size").and_then(|v| v.as_i64()).unwrap_or(0);

    if file_id.is_empty() || index < 0 {
        respond(&mut wr, 400, "Bad Request", b"fileId or index is missing or invalid", "text/plain").await;
        return;
    }

    let saved_size = file_part.body.len() as u64;
    if expected_size > 0 && saved_size as i64 != expected_size {
        let msg = format!("Chunk {index} size mismatch: expected {expected_size}, received {saved_size}");
        respond(&mut wr, 400, "Bad Request", msg.as_bytes(), "text/plain").await;
        return;
    }

    let dir = ctx.data_dir.join("upload_tmp").join(&file_id);
    if let Err(e) = tokio::fs::create_dir_all(&dir).await {
        let msg = format!("create_dir_all failed: {e}");
        respond(&mut wr, 500, "Internal Server Error", msg.as_bytes(), "text/plain").await;
        return;
    }
    let chunk_path = dir.join(format!("chunk_{index}"));
    if let Err(e) = tokio::fs::write(&chunk_path, file_part.body).await {
        let msg = format!("chunk write failed: {e}");
        respond(&mut wr, 500, "Internal Server Error", msg.as_bytes(), "text/plain").await;
        return;
    }
    let final_size = tokio::fs::metadata(&chunk_path).await.map(|m| m.len()).unwrap_or(0);
    if expected_size > 0 && final_size as i64 != expected_size {
        let _ = tokio::fs::remove_file(&chunk_path).await;
        let msg = format!("Chunk {index} final size mismatch: expected {expected_size}, saved {final_size}");
        respond(&mut wr, 400, "Bad Request", msg.as_bytes(), "text/plain").await;
        return;
    }

    let body = format!("{index}:{final_size}");
    respond(&mut wr, 201, "Created", body.as_bytes(), "text/plain").await;
}

// ── Multipart parsing (sync, operates on a fully-buffered body) ─────────────

struct Part<'a> {
    name: String,
    filename: Option<String>,
    content_type: Option<String>,
    body: &'a [u8],
}

fn parse_multipart<'a>(body: &'a [u8], boundary: &[u8]) -> Result<Vec<Part<'a>>, String> {
    let boundary_line: Vec<u8> = {
        let mut v = Vec::with_capacity(boundary.len() + 2);
        v.extend_from_slice(b"--");
        v.extend_from_slice(boundary);
        v
    };
    let part_sep: Vec<u8> = {
        let mut v = Vec::with_capacity(boundary_line.len() + 2);
        v.extend_from_slice(b"\r\n");
        v.extend_from_slice(&boundary_line);
        v
    };

    // Find the first boundary (start of first part).
    let first = find_subsequence(body, &boundary_line)
        .ok_or_else(|| "multipart: first boundary not found".to_string())?;
    let mut pos = first + boundary_line.len();
    // Skip the optional \r\n that immediately follows the first boundary.
    if body.get(pos..pos + 2) == Some(b"\r\n") {
        pos += 2;
    }

    let mut parts: Vec<Part> = Vec::new();
    loop {
        // Read part headers until \r\n\r\n.
        let header_end = find_subsequence(&body[pos..], b"\r\n\r\n")
            .ok_or_else(|| "multipart: missing part header terminator".to_string())?;
        let header_block = &body[pos..pos + header_end];
        pos += header_end + 4;

        let (name, filename, content_type) = parse_part_headers(header_block)?;
        if name.is_empty() {
            return Err("multipart: part missing name".to_string());
        }

        // Read part body until the next part separator.
        let next_sep = find_subsequence(&body[pos..], &part_sep).unwrap_or(body.len() - pos);
        let part_body = &body[pos..pos + next_sep];
        pos += next_sep;

        // Strip a trailing \r\n that always precedes the next part separator.
        let part_body = strip_trailing_crlf(part_body);

        parts.push(Part { name, filename, content_type, body: part_body });

        if pos >= body.len() {
            return Err("multipart: unexpected end of body".to_string());
        }
        // After consuming the part body, the next bytes are the part_sep's
        // leading `\r\n`, then `--{boundary}`. Skip the leading `\r\n` so
        // pos is at the start of `--{boundary}`.
        if body.get(pos..pos + 2) == Some(b"\r\n") {
            pos += 2;
        }
        // The boundary marker is always preceded by `--` (which is what
        // we just advanced past), so we can't disambiguate final vs not
        // from this position. Look at what comes *after* the full
        // `--{boundary}`:
        //   * non-final: `\r\n<next part's headers>` → check for `\r\n`
        //   * final:     `--\r\n` or end of body     → check for `--`
        if body.get(pos + boundary_line.len()..pos + boundary_line.len() + 2) == Some(b"--") {
            // Final boundary reached.
            break;
        }
        // Otherwise skip the entire `--{boundary}\r\n` to land on the
        // next part's header block.
        pos += boundary_line.len() + 2;
    }
    Ok(parts)
}

fn strip_trailing_crlf(b: &[u8]) -> &[u8] {
    if b.ends_with(b"\r\n") { &b[..b.len() - 2] } else { b }
}

fn parse_part_headers(header_block: &[u8]) -> Result<(String, Option<String>, Option<String>), String> {
    let text = std::str::from_utf8(header_block)
        .map_err(|_| "multipart: header is not utf-8".to_string())?;
    let mut name = String::new();
    let mut filename: Option<String> = None;
    let mut content_type: Option<String> = None;
    for line in text.split('\n') {
        let line = line.trim_end_matches('\r');
        if let Some((k, v)) = line.split_once(':') {
            let k = k.trim().to_ascii_lowercase();
            let v = v.trim();
            if k == "content-disposition" {
                for part in v.split(';').skip(1) {
                    let part = part.trim();
                    if let Some(n) = part.strip_prefix("name=") {
                        name = n.trim_matches('"').to_string();
                    } else if let Some(f) = part.strip_prefix("filename=") {
                        filename = Some(f.trim_matches('"').to_string());
                    }
                }
            } else if k == "content-type" {
                content_type = Some(v.to_string());
            }
        }
    }
    Ok((name, filename, content_type))
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    if needle.len() > haystack.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn parse_multipart_boundary(content_type: &str) -> Option<Vec<u8>> {
    for part in content_type.split(';').skip(1) {
        let trimmed = part.trim();
        if let Some(b) = trimmed.strip_prefix("boundary=") {
            let b = b.trim_matches('"');
            return Some(b.as_bytes().to_vec());
        }
    }
    None
}

/// Read exactly `len` bytes from `rd`.
///
/// We use the HTTP `Content-Length` from the request instead of reading
/// until EOF. With HTTP/1.1 keep-alive (which the browser uses by default
/// for XHR), EOF only arrives when the *connection* closes — not when
/// the *request body* ends. Reading until EOF would hang indefinitely
/// on the next request's first byte.
async fn read_body_exact<R: AsyncRead + Unpin>(mut rd: R, len: usize) -> Result<Vec<u8>, String> {
    if len == 0 {
        return Ok(Vec::new());
    }
    let mut buf = vec![0u8; len];
    rd.read_exact(&mut buf).await.map_err(|e| e.to_string())?;
    Ok(buf)
}

async fn stage_to_temp(ctx: &Arc<AppCtx>, data: &[u8]) -> Result<PathBuf, String> {
    let dir = ctx.data_dir.join("upload_tmp");
    tokio::fs::create_dir_all(&dir).await.map_err(|e| e.to_string())?;
    let path = dir.join(format!("upload_{}_{}.bin", std::process::id(), now_ms()));
    tokio::fs::write(&path, data).await.map_err(|e| e.to_string())?;
    Ok(path)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    type MultipartPart<'a> = (&'a str, Option<&'a str>, Option<&'a str>, &'a [u8]);

    fn build_multipart(parts: &[MultipartPart<'_>]) -> (Vec<u8>, Vec<u8>) {
        let boundary: Vec<u8> = b"----TestBoundary123".to_vec();
        let mut body = Vec::new();
        for (i, (name, filename, ctype, data)) in parts.iter().enumerate() {
            if i == 0 {
                body.extend_from_slice(b"--");
                body.extend_from_slice(&boundary);
                body.extend_from_slice(b"\r\n");
            } else {
                body.extend_from_slice(b"\r\n--");
                body.extend_from_slice(&boundary);
                body.extend_from_slice(b"\r\n");
            }
            body.extend_from_slice(b"Content-Disposition: form-data; name=\"");
            body.extend_from_slice(name.as_bytes());
            if let Some(fname) = filename {
                body.extend_from_slice(b"\"; filename=\"");
                body.extend_from_slice(fname.as_bytes());
                body.extend_from_slice(b"\"");
            }
            body.extend_from_slice(b"\"\r\n");
            if let Some(ct) = ctype {
                body.extend_from_slice(b"Content-Type: ");
                body.extend_from_slice(ct.as_bytes());
                body.extend_from_slice(b"\r\n");
            }
            body.extend_from_slice(b"\r\n");
            body.extend_from_slice(data);
        }
        body.extend_from_slice(b"\r\n--");
        body.extend_from_slice(&boundary);
        body.extend_from_slice(b"--\r\n");
        (body, boundary)
    }

    #[test]
    fn parse_multipart_two_parts() {
        let info_data = b"\x01\x02\x03\x04info-bytes-encrypted";
        let file_data = b"\x89PNG\r\n\x1a\nfake-png-content";
        let (body, boundary) = build_multipart(&[
            ("info", None, None, info_data),
            ("file", Some("cat.png"), Some("image/png"), file_data),
        ]);
        let parts = parse_multipart(&body, &boundary).expect("parse");
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].name, "info");
        assert_eq!(parts[0].body, info_data);
        assert_eq!(parts[1].name, "file");
        assert_eq!(parts[1].filename.as_deref(), Some("cat.png"));
        assert_eq!(parts[1].content_type.as_deref(), Some("image/png"));
        assert_eq!(parts[1].body, file_data);
    }

    #[test]
    fn parse_multipart_three_parts_roundtrip() {
        let (body, boundary) = build_multipart(&[
            ("info", None, None, b"enc-info"),
            ("file", Some("a.txt"), Some("text/plain"), b"alpha"),
            ("extra", None, None, b"beta"),
        ]);
        let parts = parse_multipart(&body, &boundary).expect("parse");
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].name, "info");
        assert_eq!(parts[0].body, b"enc-info");
        assert_eq!(parts[1].name, "file");
        assert_eq!(parts[1].body, b"alpha");
        assert_eq!(parts[2].name, "extra");
        assert_eq!(parts[2].body, b"beta");
    }

    #[test]
    fn parse_multipart_file_body_containing_dashes() {
        // Edge case: file body happens to contain "--" near the end, but
        // not the full boundary. The parser must not get confused.
        let info_data = b"info-payload";
        let mut file_data = Vec::new();
        file_data.extend_from_slice(b"head-");
        file_data.extend_from_slice(b"--TestB");
        file_data.extend_from_slice(b"-other"); // not the full boundary
        let (body, boundary) = build_multipart(&[
            ("info", None, None, info_data),
            ("file", Some("x.bin"), Some("application/octet-stream"), &file_data),
        ]);
        let parts = parse_multipart(&body, &boundary).expect("parse");
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].body, info_data);
        assert_eq!(parts[1].body, file_data);
    }

    #[test]
    fn parse_multipart_only_info_part() {
        let (body, boundary) = build_multipart(&[("info", None, None, b"hello")]);
        let parts = parse_multipart(&body, &boundary).expect("parse");
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].body, b"hello");
    }

    #[tokio::test]
    async fn read_body_exact_reads_content_length_bytes() {
        // Simulates a request body that does NOT end with a connection
        // close — keep-alive style. With the old read-until-EOF logic
        // this would hang forever; `read_body_exact` must return after
        // exactly `len` bytes.
        let payload: Vec<u8> = (0..4096u32).map(|i| (i & 0xff) as u8).collect();
        let mut padded = payload.clone();
        // Append "next request" bytes that must NOT be consumed.
        padded.extend_from_slice(b"GET /foo HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n");

        let mut async_reader = tokio::io::BufReader::new(padded.as_slice());
        let got = read_body_exact(&mut async_reader, payload.len()).await;
        assert_eq!(got.unwrap(), payload);
    }

    #[tokio::test]
    async fn read_body_exact_zero_length_returns_empty() {
        let empty: &[u8] = &[];
        let mut async_reader = tokio::io::BufReader::new(empty);
        let got = read_body_exact(&mut async_reader, 0).await;
        assert_eq!(got.unwrap(), Vec::<u8>::new());
    }
}
