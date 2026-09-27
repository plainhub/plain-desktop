//! `/fs` — serve a file from the local data store.
//!
//! Mirrors `plain-app` `web/routes/FilesRoutes.kt::addFilesRoutes().get("/fs")`:
//!   1. URL-decode the `id` query param.
//!   2. Base64-decode + XChaCha20-decrypt with the local server's URL
//!      token (this is how the web client delivers the path — see
//!      `getFileId` in `lib/api/file.ts`).
//!   3. Parse the decrypted payload: either a JSON object
//!      `{"path":"…","mediaId":"…","name":"…"}` or a plain URI string
//!      such as `fid:{sha256}.{ext}` / `app://…` / absolute path.
//!   4. Resolve to a real on-disk path. For `fid:` the resolution is
//!      `{data_dir}/files/{aa}/{bb}/{hash}.{ext}` — matches what
//!      `app_file_store::import_file` writes.
//!   5. Byte-range short-circuit (`?offset=…&length=…`) for BLE
//!      transports — serves raw `application/octet-stream` bytes.
//!   6. Otherwise stream the file body with RFC 5987
//!      `Content-Disposition`, honoring HTTP `Range` headers (RFC 7233)
//!      so browsers can seek media.

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum::response::Response;
use std::io::SeekFrom;
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use super::response::{cors_header_pairs, respond};
use super::uri::{parse_decrypted_id, resolve_uri};
use crate::base64_decode;
use crate::api::context::AppCtx;
use crate::api::server::ServerState;
use crate::mime::mime_from_ext;
use crate::query::parse_query;
use crate::utils::async_read_stream::AsyncReadStream;
use crate::utils::http::RangeParse;
use crate::xchacha_decrypt;

pub async fn fs_handler(State(state): State<ServerState>, req: Request) -> Response {
    let query_str = req.uri().query().unwrap_or("").to_string();
    let range_header = req
        .headers()
        .get("range")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    serve_file(&query_str, &range_header, req.headers(), &state.ctx).await
}

pub async fn serve_file(
    query_str: &str,
    range_header: &str,
    headers: &axum::http::HeaderMap,
    ctx: &Arc<AppCtx>,
) -> Response {
    // 1. Parse query params.
    let params = parse_query(query_str);
    let id_encoded = match params.get("id") {
        Some(s) if !s.is_empty() => s.clone(),
        _ => return respond(400, b"missing id".to_vec(), "text/plain"),
    };

    // 2. Decrypt the id.
    let id_bytes = base64_decode(&id_encoded);
    let Some(plaintext) = xchacha_decrypt(&ctx.token, &id_bytes) else {
        return respond(401, Vec::new(), "text/plain");
    };
    let plaintext = match String::from_utf8(plaintext) {
        Ok(s) => s,
        Err(_) => {
            return respond(
                400,
                b"decrypted id is not valid utf-8".to_vec(),
                "text/plain",
            );
        }
    };

    // 3. Parse the decrypted payload.
    let (path, json_name) = parse_decrypted_id(&plaintext);

    // 4. Resolve to a real path.
    let resolved = resolve_uri(&path, &ctx.data_dir);

    // 5. Sanity-check the file is on disk and is a file.
    let metadata = match tokio::fs::metadata(&resolved).await {
        Ok(m) => m,
        Err(_) => return respond(404, Vec::new(), "text/plain"),
    };
    if !metadata.is_file() {
        return respond(400, b"not a file".to_vec(), "text/plain");
    }
    let file_size = metadata.len();

    // 5b. Thumbnail request (`?w=…&h=…`): generate through the shared
    //     thumbnail engine (media feature) — ETag/304 fast path, LRU +
    //     file cache, small-image passthrough. Failures answer 204 No
    //     Content (the "no thumbnail available" contract), never 5xx.
    #[cfg(feature = "media")]
    if let (Some(w), Some(h)) = (
        params.get("w").and_then(|s| s.parse::<i32>().ok()),
        params.get("h").and_then(|s| s.parse::<i32>().ok()),
    ) {
        let quality = params
            .get("q")
            .and_then(|s| s.parse::<i32>().ok())
            .unwrap_or(75);
        return serve_thumbnail(&resolved, &metadata, w, h, quality, headers, ctx).await;
    }

    // 6. BLE byte-range request: `?offset=…&length=…`. Mirrors plain-app
    //    `FilesRoutes.kt`'s `readFileRange(path, rangeOffset, rangeLength)`
    //    branch — used by low-throughput transports (BLE) to download a
    //    file in small chunks. Only applies when `length > 0`; serves raw
    //    `application/octet-stream` bytes with no Content-Disposition,
    //    no thumbnails, no conversion. A request past EOF responds 404
    //    (matching Android's `readFileRange == null` path).
    if let (Some(off), Some(len)) = (
        params.get("offset").and_then(|s| s.parse::<u64>().ok()),
        params.get("length").and_then(|s| s.parse::<u64>().ok()),
    ) && len > 0
    {
        if off >= file_size {
            return respond(404, Vec::new(), "text/plain");
        }
        let clamped = len.min(file_size - off);
        return range_raw_response(&resolved, off, clamped).await;
    }

    // 7. Display filename + MIME + Content-Disposition (RFC 5987).
    //    plain-app URL-encodes the filename for both the legacy
    //    `filename="…"` and the `filename*=utf-8''…` forms — we match
    //    that exactly so non-ASCII names round-trip correctly.
    let display_name = if !json_name.is_empty() {
        json_name
    } else {
        resolved
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("file")
            .to_string()
    };
    let mime = mime_from_ext(&display_name);
    let is_download = params.get("dl").map(|s| s.as_str()) == Some("1");
    let disposition_kind = if is_download { "attachment" } else { "inline" };
    let disposition = crate::utils::http::content_disposition(disposition_kind, &display_name);

    // 8. HTTP `Range` header (RFC 7233). Only single-range requests are
    //    honored; multi-range falls through to a full 200. A syntactically
    //    valid but unsatisfiable range answers 416 with
    //    `content-range: bytes */<size>` (RFC 7233 §4.4), the same shape
    //    Ktor serves.
    match crate::utils::http::parse_range_header(range_header, file_size) {
        RangeParse::Partial(start, end) => {
            return partial_response(&resolved, start, end, file_size, mime, &disposition).await;
        }
        RangeParse::Unsatisfiable => return unsatisfiable_range_response(file_size),
        RangeParse::Full => {}
    }

    // 9. Full response (200) with `accept-ranges: bytes` so clients know
    //    they can issue `Range` requests on subsequent calls.
    full_response(&resolved, file_size, mime, &disposition).await
}

/// CORS + range-negotiation headers shared by every streaming variant.
fn streaming_common_headers(
    builder: axum::http::response::Builder,
) -> axum::http::response::Builder {
    builder.header("accept-ranges", "bytes").header(
        "access-control-expose-headers",
        "content-disposition, accept-ranges, content-range",
    )
}

/// Full `200` streaming response with the exact header set the
/// hand-rolled server sent.
pub async fn full_response(path: &Path, file_size: u64, mime: &str, disposition: &str) -> Response {
    let reader = match open_seeking_reader(path, 0, file_size).await {
        Ok(r) => r,
        Err(_) => return respond(404, Vec::new(), "text/plain"),
    };
    let mut builder = axum::http::Response::builder()
        .status(StatusCode::OK)
        .header("content-type", mime)
        .header("content-length", file_size)
        .header("content-disposition", disposition);
    builder = streaming_common_headers(builder);
    builder = with_cors(builder);
    builder
        .body(Body::from_stream(AsyncReadStream::new(reader)))
        .expect("static response")
}

/// `206 Partial Content` for an HTTP `Range` request.
pub async fn partial_response(
    path: &Path,
    start: u64,
    end: u64,
    file_size: u64,
    mime: &str,
    disposition: &str,
) -> Response {
    let length = end - start + 1;
    let reader = match open_seeking_reader(path, start, length).await {
        Ok(r) => r,
        Err(_) => return respond(404, Vec::new(), "text/plain"),
    };
    let mut builder = axum::http::Response::builder()
        .status(StatusCode::PARTIAL_CONTENT)
        .header("content-type", mime)
        .header("content-length", length)
        .header("content-range", format!("bytes {start}-{end}/{file_size}"))
        .header("content-disposition", disposition);
    builder = streaming_common_headers(builder);
    builder = with_cors(builder);
    builder
        .body(Body::from_stream(AsyncReadStream::new(reader)))
        .expect("static response")
}

/// `416 Range Not Satisfiable` (RFC 7233 §4.4): empty body, and
/// `content-range` advertises the actual size so the client can recompute
/// a valid range.
pub fn unsatisfiable_range_response(file_size: u64) -> Response {
    let mut builder = axum::http::Response::builder()
        .status(StatusCode::RANGE_NOT_SATISFIABLE)
        .header("content-length", 0)
        .header("content-range", format!("bytes */{file_size}"));
    builder = streaming_common_headers(builder);
    builder = with_cors(builder);
    builder.body(Body::empty()).expect("static response")
}

/// Raw byte range for BLE transport. Content-Type is
/// `application/octet-stream` (matches plain-app), with no
/// Content-Disposition and no Range negotiation — the caller has already
/// validated `offset` / `length`.
async fn range_raw_response(path: &Path, offset: u64, length: u64) -> Response {
    let reader = match open_seeking_reader(path, offset, length).await {
        Ok(r) => r,
        Err(_) => return respond(404, Vec::new(), "text/plain"),
    };
    let mut builder = axum::http::Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/octet-stream")
        .header("content-length", length);
    builder = with_cors(builder);
    builder
        .body(Body::from_stream(AsyncReadStream::new(reader)))
        .expect("static response")
}

/// Open `path`, seek to `offset`, and cap the reader at `length` bytes.
/// The file's existence was already verified by the metadata check in
/// [`serve_file`]; an open failure here is a race and answers 404.
async fn open_seeking_reader(
    path: &Path,
    offset: u64,
    length: u64,
) -> std::io::Result<tokio::io::Take<tokio::fs::File>> {
    let mut file = tokio::fs::File::open(path).await?;
    if offset > 0 {
        file.seek(SeekFrom::Start(offset)).await?;
    }
    Ok(file.take(length))
}

fn with_cors(builder: axum::http::response::Builder) -> axum::http::response::Builder {
    let mut builder = builder;
    for (k, v) in cors_header_pairs() {
        builder = builder.header(
            HeaderName::from_bytes(k.as_bytes()).expect("valid cors name"),
            HeaderValue::from_str(v).expect("valid cors value"),
        );
    }
    builder
}

/// Thumbnail serving via the shared pure-Rust thumbnail engine.
///
/// Flow: ETag/If-None-Match fast path (304 without any generation work —
/// the tag derives from the deterministic cache key) → engine (LRU →
/// file cache → single-flight → admission-controlled decode pipeline) →
/// small-image passthrough for originals that already fit.
///
/// Failures stay 204 No Content (the "no thumbnail available" contract
/// rather than an error the UI must handle).
#[cfg(feature = "media")]
async fn serve_thumbnail(
    path: &Path,
    meta: &std::fs::Metadata,
    w: i32,
    h: i32,
    quality: i32,
    headers: &axum::http::HeaderMap,
    ctx: &Arc<AppCtx>,
) -> Response {
    use crate::media::thumb::{self, ThumbOutcome, ThumbSpec};

    let w_n = w.clamp(0, thumb::MAX_TARGET_DIM as i32) as u32;
    let h_n = h.clamp(0, thumb::MAX_TARGET_DIM as i32) as u32;

    // Track recent file for larger thumbnails (media views).
    if w_n > 200 || h_n > 200 {
        let _ = crate::media::kv::recent::add_recent_file(&ctx.prefs, &path.to_string_lossy());
    }

    let mod_unix = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let spec = ThumbSpec::sanitize(
        &path.to_string_lossy(),
        w,
        h,
        quality,
        mod_unix,
        meta.len() as i64,
    );

    let cache_dir = crate::media::paths::detect().cache_dir;
    let cache_path = thumb::thumb_cache_path(
        &cache_dir,
        &path.to_string_lossy(),
        spec.w,
        spec.h,
        spec.quality,
        mod_unix,
        spec.file_size,
    );
    let etag = thumb::cache_etag(&cache_path);

    // Conditional request: the ETag is a hash of every invalidation input,
    // so a match proves the cached thumbnail is current.
    if let Some(inm) = headers
        .get(axum::http::header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
    {
        if inm.split(',').any(|t| t.trim() == etag) {
            return axum::http::Response::builder()
                .status(StatusCode::NOT_MODIFIED)
                .header("etag", etag)
                .header("cache-control", "private, max-age=300")
                .body(Body::empty())
                .expect("static response");
        }
    }

    // Zero-copy adapter so `Bytes::from_owner` can hold the engine's shared
    // thumbnail buffer without cloning it into the response body.
    struct SharedBytes(std::sync::Arc<Vec<u8>>);
    impl AsRef<[u8]> for SharedBytes {
        fn as_ref(&self) -> &[u8] {
            &self.0
        }
    }

    match thumb::get_thumbnail(spec).await {
        Ok(ThumbOutcome::Generated(data)) => axum::http::Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "image/jpeg")
            .header("etag", etag)
            .header("cache-control", "private, max-age=300")
            .header("content-length", data.len())
            .body(Body::from(bytes::Bytes::from_owner(SharedBytes(data))))
            .expect("static response"),
        Ok(ThumbOutcome::Original { data, mime }) => axum::http::Response::builder()
            .status(StatusCode::OK)
            .header("content-type", mime)
            .header("etag", etag)
            .header("cache-control", "private, max-age=300")
            .header("content-length", data.len())
            .body(Body::from(bytes::Bytes::from_owner(SharedBytes(data))))
            .expect("static response"),
        Err(_) => axum::http::Response::builder()
            .status(StatusCode::NO_CONTENT)
            .body(Body::empty())
            .expect("static response"),
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/api/server/file_server.rs"]
mod tests;
