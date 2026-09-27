//! `/fs` endpoint. Streams a file from disk with optional `Range` support.
//! Mirrors Go `fsHandler` in `cmd/services/api/fs.go`, aligned with
//! plain-app `FileServer.kt` for the bits the shared web client exercises:
//! `probe=1` codec negotiation, `tr=1` HEVC→H.264 transcoding and
//! animated-image/SVG passthrough.
//!
//! The frontend sends `?id=<encrypted_file_id>` along with optional
//! thumbnail params (`w`, `h`, `cc`, `q`) and display params (`preview`,
//! `name`, `dl`). The id is decrypted via `path_from_file_id` to get the
//! real filesystem path.

use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use futures::stream::Stream;
use serde::Deserialize;
use std::pin::Pin;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncSeekExt;

use crate::api::auth::AppState;
use plain_rs::utils::async_read_stream::AsyncReadStream;

#[derive(Deserialize)]
pub struct FsQuery {
    id: Option<String>,
    /// Thumbnail width.
    w: Option<i32>,
    /// Thumbnail height.
    h: Option<i32>,
    /// Thumbnail quality (1-100, default 75).
    q: Option<i32>,
    /// "1" = request thumbnail (cc flag).
    cc: Option<String>,
    /// Preview mode: "pdf" for PDF preview.
    preview: Option<String>,
    /// Override filename for Content-Disposition.
    name: Option<String>,
    /// "1" = force download (attachment) instead of inline.
    dl: Option<String>,
    /// "1" = codec probe: respond `{"codec":"<fourcc>"}` instead of bytes.
    probe: Option<String>,
    /// "1" = request a browser-playable H.264 transcode (HEVC fallback).
    tr: Option<String>,
}

type ByteStream = Pin<Box<dyn Stream<Item = Result<axum::body::Bytes, std::io::Error>> + Send>>;

pub async fn fs_handler(
    State(state): State<AppState>,
    Query(q): Query<FsQuery>,
    headers: HeaderMap,
) -> Response {
    // Resolve file path from encrypted id. Mirrors Go `resolveFSFile`:
    // only `?id=` is accepted; missing/empty → 400, decrypt failure → 403,
    // stat failure → 404, directory → 400.
    let id = q.id.as_deref().unwrap_or("").trim();
    if id.is_empty() {
        return (StatusCode::BAD_REQUEST, "").into_response();
    }
    let path = match crate::fsx::path_from_file_id(id, &state.prefs) {
        Ok(p) => p,
        Err(_) => {
            return (StatusCode::FORBIDDEN, "File is expired or does not exist.").into_response();
        }
    };
    // Chat attachments address their content by `fid:{sha256}[.{ext}]` —
    // resolve into the app-file store under the data dir (plain-app
    // `String.getFinalPath()` parity).
    let path = if path.starts_with("fid:") {
        let suffix = path.strip_prefix("fid:").unwrap_or("");
        let (hash, ext) = match suffix.split_once('.') {
            Some((h, e)) => (h, e),
            None => (suffix, ""),
        };
        plain_rs::chat::app_file_store::dest_path(&state.chat.service.data_dir, hash, ext)
            .to_string_lossy()
            .to_string()
    } else {
        path
    };

    let meta = match tokio::fs::metadata(&path).await {
        Ok(m) => m,
        Err(_) => return (StatusCode::NOT_FOUND, "").into_response(),
    };
    if meta.is_dir() {
        return (StatusCode::BAD_REQUEST, "").into_response();
    }

    // Codec probe (mirrors plain-app `probe=1`): let the web client learn the
    // video codec before committing to a playback URL — HEVC files get a
    // `tr=1` transcoded URL on browsers without an HEVC decoder.
    if q.probe.as_deref() == Some("1") {
        let codec = crate::media::video::probe_video_codec(&path).await;
        return (
            [(header::CONTENT_TYPE, "application/json")],
            format!(r#"{{"codec":"{codec}"}}"#),
        )
            .into_response();
    }

    // Build Content-Disposition header. Mirrors Go `resolveFSFileName`
    // + `setContentDispositionHeaders`. PDF preview rewrites the filename
    // extension to `.pdf` so browser viewers don't try to inline raw DOC.
    let preview = q.preview.as_deref().unwrap_or("").trim().to_lowercase();
    let file_name = resolve_file_name(&q, &path, &preview);
    let disposition = if q.dl.as_deref() == Some("1") {
        "attachment"
    } else {
        "inline"
    };
    let cd_header = plain_rs::utils::http::content_disposition(disposition, &file_name);
    let mime = crate::fsx::guess_mime(std::path::Path::new(&path));

    // PDF preview mode.
    if preview == "pdf" {
        return serve_pdf_preview(&path, &meta, &cd_header).await;
    }

    // Animated images (GIF, animated WebP, animated HEIF) and SVG: serve
    // as-is so the browser renders them natively, thumbnails included —
    // mirrors plain-app FileServer, whose thumbnail pipeline also cannot
    // decode them.
    if crate::fsx::is_animated_image_or_svg(std::path::Path::new(&path)) {
        return serve_file(&path, &meta, &mime, &cd_header, &headers).await;
    }

    // Thumbnail request?
    let w = q.w.unwrap_or(0);
    let h = q.h.unwrap_or(0);
    let cc = q.cc.as_deref() == Some("1");

    if w > 0 || h > 0 || cc {
        return serve_thumbnail(&path, &meta, w, h, q.q.unwrap_or(75), &cd_header, &headers).await;
    }

    // Track recent file — once per *view*, not per range chunk. Browsers
    // fetch video in many ~2 MiB chunks and the tracker rewrites a
    // 500-entry JSON list per call, so recording every continuation chunk
    // is O(list) KV churn on the hot path (and SD-card journal writes on
    // soft-router class hardware). "View start" = no Range header, or a
    // range that begins at byte 0.
    let range_start_zero = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .map(|r| {
            use plain_rs::utils::http::RangeParse;
            matches!(
                plain_rs::utils::http::parse_range_header(r, u64::MAX),
                RangeParse::Full | RangeParse::Partial(0, _)
            )
        })
        .unwrap_or(true);
    if range_start_zero {
        crate::db::recent::add_recent_file(&state.prefs, &path);
    }

    // Client explicitly asked for a browser-playable transcode (it probed
    // HEVC as unsupported). HEVC→H.264 is expensive, so this only runs on
    // opt-in; failures surface as 415 so the client can fall back to the
    // download hint instead of silently playing audio-only.
    if q.tr.as_deref() == Some("1") && mime == "video/mp4" {
        let codec = crate::media::video::probe_video_codec(&path).await;
        if codec == "hvc1" || codec == "hev1" {
            return match crate::media::video::transcode_mp4_for_browser(&path).await {
                Ok(transcoded_path) => {
                    let Ok(transcoded_meta) = tokio::fs::metadata(&transcoded_path).await else {
                        return (StatusCode::NOT_FOUND, "transcode not found").into_response();
                    };
                    serve_file(
                        &transcoded_path.to_string_lossy(),
                        &transcoded_meta,
                        &mime,
                        &cd_header,
                        &headers,
                    )
                    .await
                }
                Err(_) => (
                    StatusCode::UNSUPPORTED_MEDIA_TYPE,
                    "video transcoding is not available for this file",
                )
                    .into_response(),
            };
        }
        // Not HEVC: every browser can decode it — fall through.
    }

    // Serve original file with Range support.
    serve_file(&path, &meta, &mime, &cd_header, &headers).await
}

/// Resolve the Content-Disposition filename. Mirrors Go
/// `resolveFSFileName`: prefer `?name=` query param, fall back to the
/// path's basename. When `preview == "pdf"`, force the extension to
/// `.pdf` so the browser's PDF viewer picks it up.
fn resolve_file_name(q: &FsQuery, path: &str, preview: &str) -> String {
    let mut file_name = q
        .name
        .as_deref()
        .filter(|n| !n.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| path.rsplit('/').next().unwrap_or("file").to_string());

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

/// Serve the original file with HTTP Range support.
async fn serve_file(
    path: &str,
    meta: &std::fs::Metadata,
    mime: &str,
    cd_header: &str,
    headers: &HeaderMap,
) -> Response {
    use plain_rs::utils::http::RangeParse;

    let total = meta.len();
    let range = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .map(|r| plain_rs::utils::http::parse_range_header(r, total));

    let f = match File::open(path).await {
        Ok(f) => f,
        Err(_) => return (StatusCode::NOT_FOUND, "not found").into_response(),
    };

    if let Some(RangeParse::Unsatisfiable) = range {
        // RFC 7233 §4.4 — same response shape Ktor gives plain-app.
        return Response::builder()
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header(header::ACCEPT_RANGES, "bytes")
            .header(header::CONTENT_RANGE, format!("bytes */{total}"))
            .body(Body::empty())
            .unwrap();
    }
    if let Some(RangeParse::Partial(start, end)) = range {
        let mut f = f;
        if f.seek(std::io::SeekFrom::Start(start)).await.is_err() {
            return (StatusCode::INTERNAL_SERVER_ERROR, "seek failed").into_response();
        }
        let len = end - start + 1;
        let limited = f.take(len);
        let stream: ByteStream = Box::pin(AsyncReadStream::new(limited));
        return Response::builder()
            .status(StatusCode::PARTIAL_CONTENT)
            .header(header::CONTENT_TYPE, mime)
            .header(header::ACCEPT_RANGES, "bytes")
            .header(
                header::CONTENT_RANGE,
                format!("bytes {}-{}/{}", start, end, total),
            )
            .header(header::CONTENT_LENGTH, len)
            .header(header::CONTENT_DISPOSITION, cd_header)
            .header(
                header::ACCESS_CONTROL_EXPOSE_HEADERS,
                header::CONTENT_DISPOSITION.as_str(),
            )
            .body(Body::from_stream(stream))
            .unwrap();
    }
    let stream: ByteStream = Box::pin(AsyncReadStream::new(f));
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, total)
        .header(header::CONTENT_DISPOSITION, cd_header)
        .header(
            header::ACCESS_CONTROL_EXPOSE_HEADERS,
            header::CONTENT_DISPOSITION.as_str(),
        )
        .body(Body::from_stream(stream))
        .unwrap()
}

/// Serve a thumbnail via the pure-Rust thumbnail engine.
///
/// Flow: ETag/If-None-Match fast path (304 without any generation work —
/// the tag derives from the deterministic cache key) → engine (LRU →
/// file cache → single-flight → admission-controlled decode pipeline) →
/// small-image passthrough for originals that already fit.
///
/// Failures stay 204 No Content (matches the plain-app contract of "no
/// thumbnail available" rather than an error the UI must handle).
/// Zero-copy adapter so `Bytes::from_owner` can hold the engine's shared
/// thumbnail buffer without cloning it into the response body.
struct SharedBytes(std::sync::Arc<Vec<u8>>);
impl AsRef<[u8]> for SharedBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

async fn serve_thumbnail(
    path: &str,
    meta: &std::fs::Metadata,
    w: i32,
    h: i32,
    quality: i32,
    _cd_header: &str,
    headers: &HeaderMap,
) -> Response {
    use crate::media::thumb_engine::{self, ThumbOutcome, ThumbSpec};

    let w_n = w.clamp(0, thumb_engine::MAX_TARGET_DIM as i32) as u32;
    let h_n = h.clamp(0, thumb_engine::MAX_TARGET_DIM as i32) as u32;

    // Track recent file for larger thumbnails.
    if w_n > 200 || h_n > 200 {
        let _ = crate::db::recent::add_recent_file(crate::prefs::get_default(), path);
    }

    let mod_unix = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let spec = ThumbSpec::sanitize(path, w, h, quality, mod_unix, meta.len() as i64);

    let cache_dir = crate::consts::AppPaths::detect().cache_dir;
    let cache_path = thumb_engine::thumb_cache_path(
        &cache_dir,
        path,
        spec.w,
        spec.h,
        spec.quality,
        mod_unix,
        spec.file_size,
    );
    let etag = thumb_engine::cache_etag(&cache_path);

    // Conditional request: the ETag is a hash of every invalidation input,
    // so a match proves the cached thumbnail is current.
    if let Some(inm) = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
    {
        if inm.split(',').any(|t| t.trim() == etag) {
            return Response::builder()
                .status(StatusCode::NOT_MODIFIED)
                .header(header::ETAG, etag)
                .header(header::CACHE_CONTROL, "private, max-age=300")
                .body(Body::empty())
                .unwrap();
        }
    }

    match thumb_engine::get_thumbnail(spec).await {
        Ok(ThumbOutcome::Generated(data)) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "image/jpeg")
            .header(header::ETAG, etag)
            .header(header::CACHE_CONTROL, "private, max-age=300")
            .header(header::CONTENT_LENGTH, data.len())
            .body(Body::from(bytes::Bytes::from_owner(SharedBytes(data))))
            .unwrap(),
        Ok(ThumbOutcome::Original { data, mime }) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime)
            .header(header::ETAG, etag)
            .header(header::CACHE_CONTROL, "private, max-age=300")
            .header(header::CONTENT_LENGTH, data.len())
            .body(Body::from(bytes::Bytes::from_owner(SharedBytes(data))))
            .unwrap(),
        Err(_) => StatusCode::NO_CONTENT.into_response(),
    }
}

/// Serve a PDF preview (generated on demand).
async fn serve_pdf_preview(path: &str, meta: &std::fs::Metadata, cd_header: &str) -> Response {
    let data_dir = crate::consts::AppPaths::detect().data_dir;
    let mod_unix = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let size = meta.len() as i64;

    match crate::pdf_preview::get_or_create_pdf_preview(&data_dir, path, mod_unix, size).await {
        Ok(pdf_path) => {
            let _ = crate::db::recent::add_recent_file(crate::prefs::get_default(), path);
            let pdf_path_str = pdf_path.to_string_lossy().to_string();
            match tokio::fs::metadata(&pdf_path).await {
                Ok(pdf_meta) => {
                    let mime = crate::fsx::guess_mime(std::path::Path::new(&pdf_path_str));
                    serve_file(
                        &pdf_path_str,
                        &pdf_meta,
                        &mime,
                        cd_header,
                        &HeaderMap::new(),
                    )
                    .await
                }
                Err(_) => (StatusCode::NOT_FOUND, "preview not found").into_response(),
            }
        }
        Err(e) => {
            // Check if it's a known PreviewError variant.
            if let Some(pe) = e.downcast_ref::<crate::pdf_preview::PreviewError>() {
                match pe {
                    crate::pdf_preview::PreviewError::NotSupported => {
                        return (
                            StatusCode::BAD_REQUEST,
                            "preview not supported for this file",
                        )
                            .into_response();
                    }
                    crate::pdf_preview::PreviewError::ToolMissing => {
                        return (
                            StatusCode::NOT_IMPLEMENTED,
                            "LibreOffice is required for DOC/DOCX preview.",
                        )
                            .into_response();
                    }
                }
            }
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to generate preview",
            )
                .into_response()
        }
    }
}
#[cfg(test)]
#[path = "../../tests/unit/api/fs.rs"]
mod tests;
