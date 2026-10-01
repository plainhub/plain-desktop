//! `/media/:name` handler — serves short, DLNA-friendly media URLs.
//!
//! Mirrors Go `mediaHandler` in `cmd/services/api/media.go`:
//!
//!   1. Read `:name`, strip the first `.ext` (if any) to get the alias id.
//!   2. Look the id up via `dlna::media_alias::lookup` → `(path, mime)`.
//!   3. If not found / empty path → 404.
//!   4. `stat` the path; if missing or a directory → 404.
//!   5. Set `Content-Type` from `mime` when non-empty.
//!   6. Stream the file (HTTP Range supported via the shared `serve_file`).
//!
//! The earlier implementation generated thumbnails here — that was wrong:
//! thumbnail generation lives on `/fs` (see `super::file_server`). This
//! endpoint is purely the DLNA alias resolver.

use axum::body::Body;
use axum::extract::Path;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use futures_util::stream::Stream;
use std::pin::Pin;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncSeekExt;

use crate::utils::async_read_stream::AsyncReadStream;

type ByteStream = Pin<Box<dyn Stream<Item = Result<axum::body::Bytes, std::io::Error>> + Send>>;

pub async fn media_handler(Path(name): Path<String>) -> Response {
    let name = name.trim();
    if name.is_empty() {
        return (StatusCode::BAD_REQUEST, "missing name").into_response();
    }

    // Strip extension: Go uses the first '.', not the last.
    let id = match name.find('.') {
        Some(dot) if dot > 0 => &name[..dot],
        _ => name,
    };

    let (path, mime) = match crate::dlna_sender::media_alias::lookup(id) {
        Some(v) => v,
        None => return StatusCode::NOT_FOUND.into_response(),
    };
    if path.is_empty() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let meta = match tokio::fs::metadata(&path).await {
        Ok(m) => m,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    if meta.is_dir() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let mime_header = if !mime.is_empty() { Some(mime) } else { None };

    serve_file(&path, &meta, mime_header, &HeaderMap::new()).await
}

/// Stream a file with optional HTTP Range support. Mirrors Go `c.File(path)`
/// + Gin's underlying `http.ServeContent` behaviour, restricted to the bits
/// this endpoint needs (no Last-Modified / ETag — DLNA clients don't use
/// them and the alias registry is short-lived).
async fn serve_file(
    path: &str,
    meta: &std::fs::Metadata,
    mime: Option<String>,
    headers: &HeaderMap,
) -> Response {
    let total = meta.len();
    let mut start: u64 = 0;
    let mut end: u64 = total.saturating_sub(1);
    let mut use_range = false;
    if let Some(r) = headers.get(header::RANGE).and_then(|v| v.to_str().ok()) {
        if let Some(rest) = r.strip_prefix("bytes=") {
            if let Some((s, e)) = rest.split_once('-') {
                if let Ok(s) = s.parse::<u64>() {
                    start = s;
                    use_range = true;
                }
                if !e.is_empty() {
                    if let Ok(e) = e.parse::<u64>() {
                        end = e.min(total.saturating_sub(1));
                    }
                } else {
                    end = total.saturating_sub(1);
                }
            }
        }
    }

    let f = match File::open(path).await {
        Ok(f) => f,
        Err(_) => return (StatusCode::NOT_FOUND, "not found").into_response(),
    };
    let mime_str = mime
        .unwrap_or_else(|| crate::media::fsx::guess_mime(std::path::Path::new(path)).to_string());

    if use_range {
        let mut f = f;
        if f.seek(std::io::SeekFrom::Start(start)).await.is_err() {
            return (StatusCode::INTERNAL_SERVER_ERROR, "seek failed").into_response();
        }
        let len = end - start + 1;
        let limited = f.take(len);
        let stream: ByteStream = Box::pin(AsyncReadStream::new(limited));
        return Response::builder()
            .status(StatusCode::PARTIAL_CONTENT)
            .header(header::CONTENT_TYPE, mime_str)
            .header(header::ACCEPT_RANGES, "bytes")
            .header(
                header::CONTENT_RANGE,
                format!("bytes {}-{}/{}", start, end, total),
            )
            .header(header::CONTENT_LENGTH, len)
            .body(Body::from_stream(stream))
            .unwrap();
    }
    let stream: ByteStream = Box::pin(AsyncReadStream::new(f));
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime_str)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, total)
        .body(Body::from_stream(stream))
        .unwrap()
}
