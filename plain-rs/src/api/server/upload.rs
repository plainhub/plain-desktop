//! HTTP upload endpoints, ONE pair of handlers for both hosts.
//!
//! Mirrors `plain-app/.../web/routes/Upload.kt` and the Go
//! `cmd/services/api/upload.go`:
//!
//! - `POST /upload`        — direct upload, multipart with an `info`
//!   part (XChaCha20-encrypted JSON superset
//!   `{dir, replace, isAppFile, size}`) and a `file` part. `isAppFile`
//!   stages the stream to a temp file and imports it into the
//!   content-addressed chat store (201 body = the `fid:{sha}.{ext}`
//!   suffix); otherwise the stream lands at `dir/filename` with the
//!   nas unique-path/replace conflict semantics (201 body = the saved
//!   filename).
//! - `POST /upload_chunk`  — single chunk of a larger file, same
//!   multipart shape but `info` carries `{fileId, index, size}`; the
//!   chunk streams to `<data_dir>/upload_tmp/<fileId>/chunk_<index>`
//!   (201 body = `index:savedSize`, which the web client verifies).
//!
//! Both endpoints authenticate through the shared request-key path
//! (desktop: URL token + `c-id == ctx.identity.client_id`; nas: the
//! `c-id` session) BEFORE the multipart body is consumed. The `info`
//! part is decrypted with that key. File bodies stream to disk — no
//! whole-body buffering.

use std::path::{Path, PathBuf};

use axum::extract::{FromRequest, Request, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};

use super::request_key::{RequestKey, RequestKeyError, decrypt_body, resolve_request_key};
use super::response::respond;
use super::{AuthPolicy, ServerState};

/// Render a plain-text response with each host's historical
/// Content-Type spelling (nas: axum's `text/plain; charset=utf-8`,
/// desktop: bare `text/plain`).
#[cfg_attr(not(feature = "nas"), allow(unused_variables))]
fn plain(_state: &ServerState, status: u16, body: String) -> Response {
    respond(status, body.into_bytes(), "text/plain")
}

/// Auth for both upload routes: resolve the request key (sessions on
/// nas, token on desktop) and enforce the desktop single-client rule.
fn upload_key(state: &ServerState, headers: &HeaderMap) -> Result<(RequestKey, String), Response> {
    match resolve_request_key(state, headers, false) {
        Ok((key, cid)) => {
            if matches!(state.settings.auth, AuthPolicy::Session { .. }) {
                return Ok((key, cid));
            }
            // Desktop upload contract (`Upload.kt` against
            // `HttpServerManager.tokenCache[clientId]`): the single
            // local client's id.
            if cid != state.ctx.identity.client_id {
                return Err(respond(401, Vec::new(), "text/plain"));
            }
            Ok((key, cid))
        }
        Err(e) => Err(match e {
            RequestKeyError::MissingCid => plain(state, 400, "c-id header is missing".to_string()),
            RequestKeyError::SessionNotFound => plain(state, 401, String::new()),
            RequestKeyError::BadSessionToken => {
                plain(state, 500, "invalid session token".to_string())
            }
            RequestKeyError::DevHeaderMissing | RequestKeyError::DevTokenInvalid => {
                plain(state, 401, String::new())
            }
        }),
    }
}

/// Decrypt a multipart `info` part and parse it as JSON.
async fn read_info<T: for<'de> serde::Deserialize<'de>>(
    state: &ServerState,
    key: &RequestKey,
    field: &mut axum::extract::multipart::Field<'_>,
) -> Result<T, Response> {
    let mut bytes = Vec::new();
    loop {
        let chunk = match field.chunk().await {
            Ok(c) => c,
            Err(_) => return Err(plain(state, 400, "read info failed".to_string())),
        };
        let Some(chunk) = chunk else { break };
        bytes.extend_from_slice(&chunk);
    }
    let decrypted = match decrypt_body(key, &bytes) {
        Some(b) => b,
        None => {
            return Err(respond(401, Vec::new(), "text/plain"));
        }
    };
    match serde_json::from_slice(&decrypted) {
        Ok(v) => Ok(v),
        Err(e) => Err(plain(state, 400, format!("bad info json: {e}"))),
    }
}

/// The decrypted direct-upload info (superset of both hosts' shapes).
#[derive(Debug, Clone, serde::Deserialize)]
struct UploadInfo {
    #[serde(default)]
    dir: String,
    #[serde(default)]
    replace: bool,
    #[serde(default)]
    is_app_file: bool,
    #[serde(default)]
    size: i64,
}

/// Generate a unique filename if file exists (matches Go `makeUniquePath`).
fn make_unique_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let base = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let (stem, ext) = match base.rfind('.') {
        Some(pos) if pos > 0 => (&base[..pos], &base[pos..]),
        _ => (base, ""),
    };
    for i in 1..i32::MAX {
        let cand = parent.join(format!("{} ({}){}", stem, i, ext));
        if !cand.exists() {
            return cand;
        }
    }
    path.to_path_buf()
}

pub async fn upload_handler(State(state): State<ServerState>, req: Request) -> Response {
    let headers = req.headers().clone();
    let (key, _cid) = match upload_key(&state, &headers) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let mut multipart = match axum::extract::Multipart::from_request(req, &()).await {
        Ok(m) => m,
        Err(_) => return respond(400, b"invalid multipart request".to_vec(), "text/plain"),
    };

    let mut info: Option<UploadInfo> = None;
    let mut saved_filename = String::new();
    let mut last_dest_path = String::new();

    loop {
        let field = match multipart.next_field().await {
            Ok(f) => f,
            Err(_) => return respond(400, b"invalid multipart body".to_vec(), "text/plain"),
        };
        let Some(mut field) = field else { break };
        let name = field.name().map(|s| s.to_owned());
        match name.as_deref() {
            Some("info") => match read_info::<UploadInfo>(&state, &key, &mut field).await {
                Ok(i) => {
                    log::debug!(
                        "[/upload] info dir={:?} replace={} app={}",
                        i.dir,
                        i.replace,
                        i.is_app_file
                    );
                    info = Some(i);
                }
                Err(r) => return r,
            },
            Some("file") => {
                let upload_info = match &info {
                    Some(i) => i.clone(),
                    None => {
                        return plain(&state, 400, "info part missing before file".to_string());
                    }
                };

                let file_name = field.file_name().unwrap_or("").to_string();
                let content_type = field.content_type().map(|s| s.to_owned());
                // The nas contract requires an explicit target dir; the
                // desktop contract also accepts an empty dir (chat flows
                // default to the app data dir).
                if file_name.is_empty() {
                    return plain(&state, 400, "dir or filename missing".to_string());
                }

                if upload_info.is_app_file {
                    // Desktop chat flow: stage the stream to a temp file
                    // (the hash + dedup pipeline needs a real on-disk
                    // file), then import into the content-addressed store.
                    let temp = match stage_stream_to_temp(&state.ctx.data_dir, &mut field).await {
                        Ok((p, _len)) => p,
                        Err(r) => return r,
                    };
                    match crate::chat::app_file_store::import_file(
                        &state.ctx.db,
                        &state.ctx.data_dir,
                        &temp,
                        &file_name,
                        content_type.as_deref().unwrap_or_default(),
                    ) {
                        Ok(result) => {
                            let _ = tokio::fs::remove_file(&temp).await;
                            saved_filename = result.fid_suffix;
                            last_dest_path = temp.to_string_lossy().to_string();
                        }
                        Err(e) => {
                            let _ = tokio::fs::remove_file(&temp).await;
                            return respond(500, e.to_string().into_bytes(), "text/plain");
                        }
                    }
                } else {
                    let safe_name = Path::new(&file_name)
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("file")
                        .to_string();
                    let dest_path = if upload_info.dir.is_empty() {
                        state.ctx.data_dir.join(&safe_name)
                    } else {
                        Path::new(&upload_info.dir).join(&safe_name)
                    };
                    log::debug!(
                        "[/upload] incoming file={:?} dest={:?}",
                        file_name,
                        dest_path
                    );

                    // Handle conflict: replace or make unique path.
                    let (dest_path, file_name) = if dest_path.exists() && !dest_path.is_dir() {
                        if upload_info.replace {
                            log::debug!("[/upload] replacing existing file: {:?}", dest_path);
                            let _ = tokio::fs::remove_file(&dest_path).await;
                            (dest_path, safe_name)
                        } else {
                            let unique = make_unique_path(&dest_path);
                            let name = unique
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("file")
                                .to_string();
                            log::debug!("[/upload] target exists, using unique path: {:?}", unique);
                            (unique, name)
                        }
                    } else {
                        (dest_path, safe_name)
                    };

                    if let Some(parent) = dest_path.parent() {
                        if let Err(e) = tokio::fs::create_dir_all(parent).await {
                            return plain(&state, 400, format!("cannot create dir: {e}"));
                        }
                    }

                    let written = match stream_field_to_file(
                        &mut field,
                        &dest_path,
                        upload_info.size,
                    )
                    .await
                    {
                        Ok(n) => n,
                        Err(r) => return r,
                    };
                    if upload_info.size > 0 && written != upload_info.size as u64 {
                        let msg = format!(
                            "Size mismatch: expected {}, got {written}",
                            upload_info.size
                        );
                        return respond(400, msg.into_bytes(), "text/plain");
                    }

                    saved_filename = file_name;
                    last_dest_path = dest_path.to_string_lossy().to_string();
                    // Index the uploaded file (fire-and-forget, nas parity).
                    spawn_media_index(&state, last_dest_path.clone());
                }
            }
            _ => {
                // Drain unknown fields so the parser stays in sync.
                let _ = field.bytes().await;
            }
        }
    }

    if saved_filename.is_empty() {
        return plain(&state, 400, "no file uploaded".to_string());
    }

    log::info!(
        "[/upload] saved file={:?} path={:?}",
        saved_filename,
        last_dest_path
    );

    plain(&state, 201, saved_filename)
}

/// `POST /upload_chunk` — chunk upload (>200MB files).
///
/// Multipart parts:
///   - `info`: encrypted JSON `{ "fileId": "...", "index": N, "size": M }`
///   - `file`: the chunk binary data
pub async fn upload_chunk_handler(State(state): State<ServerState>, req: Request) -> Response {
    let headers = req.headers().clone();
    let (key, _cid) = match upload_key(&state, &headers) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let mut multipart = match axum::extract::Multipart::from_request(req, &()).await {
        Ok(m) => m,
        Err(_) => return respond(400, b"invalid multipart request".to_vec(), "text/plain"),
    };

    #[derive(Debug, serde::Deserialize)]
    struct UploadChunkInfo {
        #[serde(default)]
        file_id: String,
        #[serde(default)]
        index: Option<i64>,
        #[serde(default)]
        size: i64,
    }

    let mut info: Option<UploadChunkInfo> = None;
    let mut chunk_written: Option<(i64, u64)> = None;

    loop {
        let field = match multipart.next_field().await {
            Ok(f) => f,
            Err(_) => return respond(400, b"invalid multipart body".to_vec(), "text/plain"),
        };
        let Some(mut field) = field else { break };
        let name = field.name().map(|s| s.to_owned());
        match name.as_deref() {
            Some("info") => match read_info::<UploadChunkInfo>(&state, &key, &mut field).await {
                Ok(i) => {
                    log::debug!(
                        "[/upload_chunk] info fileId={:?} index={:?}",
                        i.file_id,
                        i.index
                    );
                    info = Some(i);
                }
                Err(r) => return r,
            },
            Some("file") => {
                let upload_info = match &info {
                    Some(i) if !i.file_id.is_empty() && i.index.is_some() => i,
                    _ => {
                        return plain(
                            &state,
                            400,
                            "fileId or index is missing or invalid".to_string(),
                        );
                    }
                };

                let file_id = upload_info.file_id.clone();
                let index = upload_info.index.unwrap_or(-1);
                if index < 0 {
                    return plain(&state, 400, "invalid chunk index".to_string());
                }

                let dir = state.ctx.data_dir.join("upload_tmp").join(&file_id);
                if let Err(e) = tokio::fs::create_dir_all(&dir).await {
                    let msg = format!("create_dir_all failed: {e}");
                    return respond(500, msg.into_bytes(), "text/plain");
                }
                let chunk_path = dir.join(format!("chunk_{index}"));
                match stream_field_to_file(&mut field, &chunk_path, upload_info.size).await {
                    Ok(_) => {}
                    Err(r) => return r,
                }
                let final_size = tokio::fs::metadata(&chunk_path)
                    .await
                    .map(|m| m.len())
                    .unwrap_or(0);
                if upload_info.size > 0 && final_size != upload_info.size as u64 {
                    let _ = tokio::fs::remove_file(&chunk_path).await;
                    let msg = format!(
                        "Chunk {index} final size mismatch: expected {}, saved {final_size}",
                        upload_info.size
                    );
                    return respond(400, msg.into_bytes(), "text/plain");
                }
                chunk_written = Some((index, final_size));
            }
            _ => {
                let _ = field.bytes().await;
            }
        }
    }

    match chunk_written {
        Some((index, final_size)) => {
            log::info!("[/upload_chunk] saved chunk_{index} ({final_size} bytes)");
            // The web client parses `index:savedSize` and verifies the
            // size (see `uploadChunk` in `lib/upload/upload.ts`).
            plain(&state, 201, format!("{index}:{final_size}"))
        }
        None => plain(&state, 400, "chunk upload failed".to_string()),
    }
}

/// Stream a multipart field's body to `dest` (created fresh), counting
/// bytes. Enforces `expected` (>0) while streaming so an oversized body
/// aborts early instead of filling the disk.
async fn stream_field_to_file(
    field: &mut axum::extract::multipart::Field<'_>,
    dest: &Path,
    expected: i64,
) -> Result<u64, Response> {
    use tokio::io::AsyncWriteExt;

    let bad_request = |msg: String| (axum::http::StatusCode::BAD_REQUEST, msg).into_response();
    let mut f = match tokio::fs::File::create(dest).await {
        Ok(f) => f,
        Err(e) => {
            let _ = tokio::fs::remove_file(dest).await;
            return Err(bad_request(format!("cannot create file: {e}")));
        }
    };
    let mut written: u64 = 0;
    loop {
        let chunk = match field.chunk().await {
            Ok(c) => c,
            Err(_) => return Err(bad_request("write file error".to_string())),
        };
        let Some(chunk) = chunk else { break };
        written += chunk.len() as u64;
        if expected > 0 && written > expected as u64 {
            drop(f);
            let _ = tokio::fs::remove_file(dest).await;
            return Err(bad_request(format!(
                "Size mismatch: expected {expected}, got {written}"
            )));
        }
        if f.write_all(&chunk).await.is_err() {
            return Err(bad_request("write file error".to_string()));
        }
    }
    let _ = f.flush().await;
    Ok(written)
}

/// Stream a multipart field into a fresh temp file under
/// `<data_dir>/upload_tmp/` — the chat `isAppFile` staging path.
async fn stage_stream_to_temp(
    data_dir: &Path,
    field: &mut axum::extract::multipart::Field<'_>,
) -> Result<(PathBuf, u64), Response> {
    let dir = data_dir.join("upload_tmp");
    if let Err(e) = tokio::fs::create_dir_all(&dir).await {
        return Err((axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response());
    }
    let path = dir.join(format!("upload_{}_{}.bin", std::process::id(), now_ms()));
    let written = stream_field_to_file(field, &path, 0).await?;
    Ok((path, written))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Fire-and-forget media indexing for a completed non-app upload
/// (feature `media`; nas parity — the desktop gains it too).
fn spawn_media_index(state: &ServerState, path: String) {
    #[cfg(feature = "media")]
    {
        if crate::media::scan::is_media_excluded(&path) {
            return;
        }
        let db = state.ctx.media.db.clone();
        tokio::spawn(async move {
            match tokio::task::spawn_blocking(move || crate::media::scan::scan_file(&db, &path))
                .await
            {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => log::error!("[/upload] index file error: {e}"),
                Err(e) => log::error!("[/upload] index task failed: {e}"),
            }
        });
    }
    #[cfg(not(feature = "media"))]
    {
        let _ = (state, path);
    }
}
