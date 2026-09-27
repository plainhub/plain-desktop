//! `/fs` — serve a file, ONE handler for both hosts.
//!
//! Mirrors `plain-app` `web/routes/FilesRoutes.kt::addFilesRoutes().get("/fs")`
//! plus the plain-nas `fs.go` additions the shared web client exercises:
//!
//!   1. Parse query params; `?id=` is required (400 when missing).
//!   2. Base64-decode + XChaCha20-decrypt the id with the local server's
//!      URL token (this is how the web client delivers the path — see
//!      `getFileId` in `lib/api/file.ts`). Decrypt failure is a 403 on
//!      the nas host, a 401 on the desktop host (each host's previous
//!      contract).
//!   3. Parse the decrypted payload: either a JSON object
//!      `{"path":"…","mediaId":"…","name":"…"}` or a plain URI string
//!      such as `fid:{sha256}.{ext}` / `app://…` / absolute path.
//!   4. Resolve to a real on-disk path. For `fid:` the resolution is
//!      `{data_dir}/files/{aa}/{bb}/{hash}.{ext}` — matches what
//!      `app_file_store::import_file` writes.
//!   5. `?probe=1` codec probe: respond `{"codec":"<fourcc>"}`.
//!   6. `?preview=pdf` LibreOffice preview (nas).
//!   7. Animated-image / SVG passthrough so browsers render them.
//!   8. Thumbnails (`?w=…&h=…&cc=1&q=…`) through the shared engine:
//!      ETag + If-None-Match → 304, failures → 204.
//!   9. BLE byte-range short-circuit (`?offset=…&length=…`) for BLE
//!      transports — raw `application/octet-stream` bytes.
//!  10. `?tr=1` HEVC→H.264 transcode with a 415 fallback.
//!  11. Otherwise stream the file body with RFC 5987
//!      `Content-Disposition`, honoring HTTP `Range` headers (RFC 7233)
//!      so browsers can seek media. Recent-file tracking fires once
//!      per view (range starting at byte 0), not per continuation chunk.

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use std::io::SeekFrom;
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use super::response::respond;
use super::uri::{parse_decrypted_id, resolve_uri};
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
    serve_file(&query_str, &range_header, req.headers(), &state).await
}

/// Host-appropriate error for an id that fails token decryption: the
/// nas contract answers 403 ("File is expired or does not exist."),
/// the desktop contract a bare 401.
#[cfg_attr(not(feature = "nas"), allow(unused_variables))]
fn forbidden(state: &ServerState) -> Response {
    #[cfg(feature = "nas")]
    if state.nas.is_some() {
        return respond(
            403,
            b"File is expired or does not exist.".to_vec(),
            "text/plain",
        );
    }
    respond(401, Vec::new(), "text/plain")
}

/// Host-appropriate 400 body: nas answers empty, the desktop carries a
/// short reason.
#[cfg_attr(not(feature = "nas"), allow(unused_variables))]
fn bad_request(state: &ServerState, desktop_msg: &'static [u8]) -> Response {
    #[cfg(feature = "nas")]
    if state.nas.is_some() {
        return respond(400, Vec::new(), "text/plain");
    }
    respond(400, desktop_msg.to_vec(), "text/plain")
}

pub async fn serve_file(
    query_str: &str,
    range_header: &str,
    headers: &HeaderMap,
    state: &ServerState,
) -> Response {
    let ctx = &state.ctx;
    // 1. Parse query params.
    let params = parse_query(query_str);
    let id_encoded = match params.get("id") {
        Some(s) if !s.is_empty() => s.clone(),
        _ => return bad_request(state, b"missing id"),
    };
    // Some URL parsers turn '+' into ' ' in query strings (plain-nas
    // `path_from_file_id` parity).
    let id_encoded = id_encoded.replace(' ', "+");

    // 2. Decrypt the id with the URL token (both hosts encrypt /fs ids
    //    with it — the desktop's ctx.token and the nas prefs url_token
    //    are the same value).
    let id_bytes = crate::base64_decode(&id_encoded);
    let Some(plaintext) = xchacha_decrypt(&ctx.token, &id_bytes) else {
        return forbidden(state);
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

    // 3-4. Parse + resolve to a real path (JSON form, fid:, app://,
    //      absolute or data-dir-relative).
    let (path, json_name) = parse_decrypted_id(&plaintext);
    let resolved = resolve_uri(&path, &ctx.data_dir);

    // 5. Sanity-check the file is on disk and is a file.
    let metadata = match tokio::fs::metadata(&resolved).await {
        Ok(m) => m,
        Err(_) => return respond(404, Vec::new(), "text/plain"),
    };
    if !metadata.is_file() {
        return bad_request(state, b"not a file");
    }
    let file_size = metadata.len();

    // 6. Codec probe (plain-app `probe=1`): let the web client learn the
    //    video codec before committing to a playback URL — HEVC files
    //    get a `tr=1` transcoded URL on browsers without an HEVC
    //    decoder.
    #[cfg(feature = "media")]
    if params.get("probe").map(String::as_str) == Some("1") {
        let codec = crate::media::video::probe_video_codec(&resolved.to_string_lossy()).await;
        return respond(
            200,
            format!(r#"{{"codec":"{codec}"}}"#).into_bytes(),
            "application/json",
        );
    }

    // 7. Display filename + MIME + Content-Disposition (RFC 5987).
    //    `?name=` overrides, `preview=pdf` rewrites the extension so
    //    browser viewers don't try to inline raw DOC. The MIME source
    //    stays per-host: the nas contract guesses from the on-disk path
    //    (charset-appended for text/*), the desktop from the display
    //    name (no charset).
    let preview = params.get("preview").map(String::as_str).unwrap_or("").trim().to_lowercase();
    let display_name = resolve_display_name(&params, &json_name, &resolved, &preview);
    let mime = response_mime(state, &display_name, &resolved);
    let is_download = params.get("dl").map(String::as_str) == Some("1");
    let disposition_kind = if is_download { "attachment" } else { "inline" };
    let disposition = crate::utils::http::content_disposition(disposition_kind, &display_name);

    // 8. PDF preview mode (nas): LibreOffice-converted copy.
    #[cfg(feature = "nas")]
    if preview == "pdf" {
        return serve_pdf_preview(&resolved, &metadata, &disposition, ctx).await;
    }

    // 9. Animated images (GIF, animated WebP, animated HEIF) and SVG:
    //    serve as-is so the browser renders them natively, thumbnails
    //    included — mirrors plain-app FileServer, whose thumbnail
    //    pipeline also cannot decode them.
    #[cfg(feature = "media")]
    if crate::media::fsx::is_animated_image_or_svg(&resolved) {
        return stream_with_range(&resolved, file_size, &metadata, &mime, &disposition, range_header)
            .await;
    }

    // 10. Thumbnail request (`?w=…&h=…&cc=1`): generate through the
    //     shared thumbnail engine — ETag/304 fast path, LRU + file
    //     cache, small-image passthrough. Failures answer 204 No
    //     Content (the "no thumbnail available" contract), never 5xx.
    #[cfg(feature = "media")]
    {
        let w = params.get("w").and_then(|s| s.parse::<i32>().ok()).unwrap_or(0);
        let h = params.get("h").and_then(|s| s.parse::<i32>().ok()).unwrap_or(0);
        let cc = params.get("cc").map(String::as_str) == Some("1");
        if w > 0 || h > 0 || cc {
            let quality = params
                .get("q")
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(75);
            return serve_thumbnail(&resolved, &metadata, w, h, quality, headers, ctx).await;
        }
    }

    // 11. BLE byte-range request: `?offset=…&length=…`. Mirrors
    //     plain-app `FilesRoutes.kt`'s `readFileRange` branch — used by
    //     low-throughput transports (BLE) to download a file in small
    //     chunks. Only applies when `length > 0`; serves raw
    //     `application/octet-stream` bytes with no Content-Disposition,
    //     no thumbnails, no conversion. A request past EOF responds 404
    //     (matching Android's `readFileRange == null` path).
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

    // 12. Track recent file — once per *view*, not per range chunk.
    //     Browsers fetch video in many ~2 MiB chunks and the tracker
    //     rewrites a 500-entry JSON list per call, so recording every
    //     continuation chunk is O(list) KV churn on the hot path (and
    //     SD-card journal writes on soft-router class hardware).
    //     "View start" = no Range header, or a range that begins at
    //     byte 0.
    #[cfg(feature = "media")]
    {
        let range_start_zero = if range_header.is_empty() {
            true
        } else {
            matches!(
                crate::utils::http::parse_range_header(range_header, u64::MAX),
                RangeParse::Full | RangeParse::Partial(0, _)
            )
        };
        if range_start_zero {
            let _ = crate::media::kv::recent::add_recent_file(&ctx.prefs, &resolved.to_string_lossy());
        }
    }

    // 13. Browser-playable transcode (`?tr=1`, plain-app parity): the
    //     client probed HEVC as unsupported. HEVC→H.264 is expensive, so
    //     this only runs on opt-in; failures surface as 415 so the
    //     client can fall back to the download hint instead of silently
    //     playing audio-only.
    #[cfg(feature = "media")]
    if params.get("tr").map(String::as_str) == Some("1") && mime == "video/mp4" {
        let codec = crate::media::video::probe_video_codec(&resolved.to_string_lossy()).await;
        if codec == "hvc1" || codec == "hev1" {
            return match crate::media::video::transcode_mp4_for_browser(&resolved.to_string_lossy())
                .await
            {
                Ok(transcoded_path) => {
                    let Ok(transcoded_meta) = tokio::fs::metadata(&transcoded_path).await else {
                        return respond(
                            404,
                            b"transcode not found".to_vec(),
                            "text/plain",
                        );
                    };
                    let transcoded_str = transcoded_path.to_string_lossy().to_string();
                    stream_with_range(
                        Path::new(&transcoded_str),
                        transcoded_meta.len(),
                        &transcoded_meta,
                        &mime,
                        &disposition,
                        range_header,
                    )
                    .await
                }
                Err(_) => respond(
                    415,
                    b"video transcoding is not available for this file".to_vec(),
                    "text/plain",
                ),
            };
        }
        // Not HEVC: every browser can decode it — fall through.
    }

    // 14. Serve original file with Range support.
    stream_with_range(&resolved, file_size, &metadata, &mime, &disposition, range_header).await
}

/// Per-host MIME resolution (nas: fsx::guess_mime over the resolved
/// path, text types carry `; charset=utf-8`; desktop: bare
/// `mime_from_ext` over the display name).
#[cfg_attr(not(feature = "nas"), allow(unused_variables))]
fn response_mime(state: &ServerState, display_name: &str, resolved: &Path) -> String {
    #[cfg(feature = "nas")]
    if state.nas.is_some() {
        return crate::media::fsx::guess_mime(resolved);
    }
    mime_from_ext(display_name).to_string()
}

/// Resolve the Content-Disposition filename. Mirrors Go
/// `resolveFSFileName`: prefer `?name=` query param, fall back to the
/// path's basename. When `preview == "pdf"`, force the extension to
/// `.pdf` so the browser's PDF viewer picks it up.
fn resolve_display_name(
    params: &std::collections::HashMap<String, String>,
    json_name: &str,
    resolved: &Path,
    preview: &str,
) -> String {
    let mut file_name = if !json_name.is_empty() {
        json_name.to_string()
    } else {
        params
            .get("name")
            .map(String::as_str)
            .filter(|n| !n.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| {
                resolved
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("file")
                    .to_string()
            })
    };

    if preview == "pdf" {
        let lower = file_name.to_lowercase();
        if !lower.ends_with(".pdf") {
            let base = match file_name.rfind('.') {
                Some(idx) if idx > 0 => &file_name[..idx],
                _ => &file_name[..],
            };
            file_name = format!("{base}.pdf");
        }
    }
    file_name
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

/// Serve the original file with HTTP Range support: 206 partial /
/// 416 unsatisfiable / 200 full.
async fn stream_with_range(
    path: &Path,
    file_size: u64,
    meta: &std::fs::Metadata,
    mime: &str,
    disposition: &str,
    range_header: &str,
) -> Response {
    let _ = meta;
    match crate::utils::http::parse_range_header(range_header, file_size) {
        RangeParse::Partial(start, end) => {
            partial_response(path, start, end, file_size, mime, disposition).await
        }
        RangeParse::Unsatisfiable => unsatisfiable_range_response(file_size),
        RangeParse::Full => full_response(path, file_size, mime, disposition).await,
    }
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
    let builder = axum::http::Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/octet-stream")
        .header("content-length", length);
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
    headers: &HeaderMap,
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

/// Serve a PDF preview (generated on demand via LibreOffice, nas host).
#[cfg(feature = "nas")]
async fn serve_pdf_preview(
    path: &Path,
    meta: &std::fs::Metadata,
    cd_header: &str,
    ctx: &Arc<AppCtx>,
) -> Response {
    let mod_unix = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let size = meta.len() as i64;

    match crate::nas::pdf_preview::get_or_create_pdf_preview(
        &ctx.data_dir,
        &path.to_string_lossy(),
        mod_unix,
        size,
    )
    .await
    {
        Ok(pdf_path) => {
            let _ = crate::media::kv::recent::add_recent_file(&ctx.prefs, &path.to_string_lossy());
            let pdf_path_str = pdf_path.to_string_lossy().to_string();
            match tokio::fs::metadata(&pdf_path).await {
                Ok(pdf_meta) => {
                    let mime = crate::media::fsx::guess_mime(Path::new(&pdf_path_str));
                    full_response(Path::new(&pdf_path_str), pdf_meta.len(), &mime, cd_header).await
                }
                Err(_) => respond(404, b"preview not found".to_vec(), "text/plain"),
            }
        }
        Err(e) => {
            // Check if it's a known PreviewError variant.
            if let Some(pe) = e.downcast_ref::<crate::nas::pdf_preview::PreviewError>() {
                match pe {
                    crate::nas::pdf_preview::PreviewError::NotSupported => {
                        return respond(
                            400,
                            b"preview not supported for this file".to_vec(),
                            "text/plain",
                        );
                    }
                    crate::nas::pdf_preview::PreviewError::ToolMissing => {
                        return respond(
                            501,
                            b"LibreOffice is required for DOC/DOCX preview.".to_vec(),
                            "text/plain",
                        );
                    }
                }
            }
            respond(
                500,
                b"failed to generate preview".to_vec(),
                "text/plain",
            )
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/api/server/file_server.rs"]
mod tests;

#[cfg(all(test, feature = "nas"))]
#[path = "../../../tests/unit/api/server/file_server_nas.rs"]
mod nas_tests;
