//! `/upload` and `/upload_chunk` handlers.
//!
//! Mirrors the Go `cmd/services/api/upload.go`:
//! - `/upload`: direct file upload (≤200MB) with encrypted `info` + `file` parts
//! - `/upload_chunk`: chunk upload (>200MB) with encrypted `info` + chunk `file`
//!
//! Protocol:
//!   Client sends `c-id` header for session lookup.
//!   Multipart form has:
//!     - `info`: ChaCha20-encrypted JSON blob
//!     - `file`: the file/chunk binary data
//!   Server decrypts `info` with session key to get upload metadata.

use axum::extract::{DefaultBodyLimit, Multipart, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

use crate::api::auth::AppState;
use crate::consts::AppPaths;
use crate::db::SessionStore;

/// Decrypted info for direct upload.
#[derive(Debug, Clone, Deserialize)]
struct UploadInfo {
    dir: String,
    #[serde(default)]
    replace: bool,
}

/// Decrypted info for chunk upload.
#[derive(Debug, Deserialize)]
struct UploadChunkInfo {
    file_id: Option<String>,
    index: Option<i32>,
}

impl UploadChunkInfo {
    fn file_id(&self) -> &str {
        self.file_id.as_deref().unwrap_or("")
    }
}

/// Get the ChaCha20 key from the `c-id` header session.
fn get_session_key(state: &AppState, headers: &HeaderMap) -> Result<[u8; 32], Response> {
    let client_id = headers
        .get("c-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if client_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "c-id header is missing").into_response());
    }

    let session = SessionStore::new(&state.db)
        .get(client_id)
        .ok_or_else(|| StatusCode::UNAUTHORIZED.into_response())?;

    let key_bytes = base64::engine::general_purpose::STANDARD
        .decode(&session.token)
        .map_err(|_| {
            (StatusCode::INTERNAL_SERVER_ERROR, "invalid session token").into_response()
        })?;

    if key_bytes.len() != 32 {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "invalid session key length",
        )
            .into_response());
    }

    let mut key = [0u8; 32];
    key.copy_from_slice(&key_bytes);
    Ok(key)
}

/// Decrypt a ChaCha20-encrypted blob and parse as JSON.
fn decrypt_info<T: for<'de> Deserialize<'de>>(key: &[u8; 32], blob: &[u8]) -> Result<T, Response> {
    let decrypted = crate::crypto::decrypt(key, blob)
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, "decrypt info failed").into_response())?;
    serde_json::from_slice(&decrypted)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("bad info json: {e}")).into_response())
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

/// `POST /upload` — direct file upload (≤200MB).
///
/// Multipart parts:
///   - `info`: ChaCha20-encrypted JSON `{ "dir": "...", "replace": bool }`
///   - `file`: the file binary (filename from Content-Disposition)
pub async fn upload_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut form: Multipart,
) -> Response {
    let key = match get_session_key(&state, &headers) {
        Ok(k) => k,
        Err(r) => return r,
    };

    let mut info: Option<UploadInfo> = None;
    let mut saved_filename = String::new();
    let mut last_dest_path = String::new();

    while let Ok(Some(field)) = form.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "info" => {
                let bytes = match field.bytes().await {
                    Ok(b) => b,
                    Err(_) => return (StatusCode::BAD_REQUEST, "read info failed").into_response(),
                };
                match decrypt_info::<UploadInfo>(&key, &bytes) {
                    Ok(i) => {
                        log::debug!("[/upload] info dir={:?} replace={}", i.dir, i.replace);
                        info = Some(i);
                    }
                    Err(r) => return r,
                }
            }
            "file" => {
                let upload_info = match &info {
                    Some(i) => i.clone(),
                    None => {
                        return (StatusCode::BAD_REQUEST, "info part missing before file")
                            .into_response();
                    }
                };

                let file_name = field.file_name().unwrap_or("").to_string();
                if upload_info.dir.is_empty() || file_name.is_empty() {
                    return (StatusCode::BAD_REQUEST, "dir or filename missing").into_response();
                }

                let dest_path = Path::new(&upload_info.dir).join(&file_name);
                let dest_path = std::path::PathBuf::from(
                    dest_path.components().collect::<std::path::PathBuf>(),
                );
                log::debug!(
                    "[/upload] incoming file={:?} dest={:?}",
                    file_name,
                    dest_path
                );

                // Handle conflict: replace or make unique path
                let (dest_path, file_name) = if dest_path.exists() && !dest_path.is_dir() {
                    if upload_info.replace {
                        log::debug!("[/upload] replacing existing file: {:?}", dest_path);
                        let _ = tokio::fs::remove_file(&dest_path).await;
                        (dest_path, file_name)
                    } else {
                        let unique = make_unique_path(&dest_path);
                        let name = unique
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("file")
                            .to_string();
                        log::debug!(
                            "[/upload] target exists, using unique path: {:?}",
                            unique
                        );
                        (unique, name)
                    }
                } else {
                    (dest_path, file_name)
                };

                // Create parent directory
                if let Some(parent) = dest_path.parent() {
                    if let Err(e) = tokio::fs::create_dir_all(parent).await {
                        return (StatusCode::BAD_REQUEST, format!("cannot create dir: {e}"))
                            .into_response();
                    }
                }

                // Write file
                let mut f = match tokio::fs::File::create(&dest_path).await {
                    Ok(f) => f,
                    Err(e) => {
                        return (StatusCode::BAD_REQUEST, format!("cannot create file: {e}"))
                            .into_response();
                    }
                };

                let mut stream = field;
                while let Ok(Some(chunk)) = stream.chunk().await {
                    if f.write_all(&chunk).await.is_err() {
                        return (StatusCode::BAD_REQUEST, "write file error").into_response();
                    }
                }
                let _ = f.flush().await;
                drop(f);

                saved_filename = file_name;
                last_dest_path = dest_path.to_string_lossy().to_string();
            }
            _ => {
                // ignore unknown parts
            }
        }
    }

    if saved_filename.is_empty() {
        return (StatusCode::BAD_REQUEST, "no file uploaded").into_response();
    }

    log::info!(
        "[/upload] saved file={:?} path={:?}",
        saved_filename,
        last_dest_path
    );

    // Index the uploaded file (fire-and-forget)
    let db = state.db.clone();
    let path_clone = last_dest_path.clone();
    tokio::spawn(async move {
        if let Err(e) = crate::media_scan::scan_file(&db, &path_clone) {
            log::error!("[/upload] index file error for {:?}: {}", path_clone, e);
        }
    });

    (StatusCode::CREATED, saved_filename).into_response()
}

/// `POST /upload_chunk` — chunk upload (>200MB files).
///
/// Multipart parts:
///   - `info`: ChaCha20-encrypted JSON `{ "fileId": "...", "index": N }`
///   - `file`: the chunk binary data
pub async fn upload_chunk_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut form: Multipart,
) -> Response {
    let key = match get_session_key(&state, &headers) {
        Ok(k) => k,
        Err(r) => return r,
    };

    let paths = AppPaths::detect();
    let mut info: Option<UploadChunkInfo> = None;
    let mut chunk_saved = false;
    let mut chunk_path_saved = String::new();

    while let Ok(Some(field)) = form.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "info" => {
                let bytes = match field.bytes().await {
                    Ok(b) => b,
                    Err(_) => return (StatusCode::BAD_REQUEST, "read info failed").into_response(),
                };
                match decrypt_info::<UploadChunkInfo>(&key, &bytes) {
                    Ok(i) => {
                        log::debug!(
                            "[/upload_chunk] info fileId={:?} index={:?}",
                            i.file_id(),
                            i.index
                        );
                        info = Some(i);
                    }
                    Err(r) => return r,
                }
            }
            "file" => {
                let upload_info = match &info {
                    Some(i) if !i.file_id().is_empty() && i.index.is_some() => i,
                    _ => {
                        return (
                            StatusCode::BAD_REQUEST,
                            "fileId or index is missing or invalid",
                        )
                            .into_response();
                    }
                };

                let file_id = upload_info.file_id().to_string();
                let index = upload_info.index.unwrap_or(-1);
                if index < 0 {
                    return (StatusCode::BAD_REQUEST, "invalid chunk index").into_response();
                }

                // Read chunk data
                let mut bytes = Vec::new();
                let mut stream = field;
                while let Ok(Some(chunk)) = stream.chunk().await {
                    bytes.extend_from_slice(&chunk);
                }

                // Save chunk
                match crate::chunked_upload::save_chunk(&paths.data_dir, &file_id, index, &bytes)
                    .await
                {
                    Ok(p) => {
                        chunk_saved = true;
                        chunk_path_saved = p.to_string_lossy().to_string();
                    }
                    Err(e) => {
                        return (StatusCode::BAD_REQUEST, format!("save chunk error: {e}"))
                            .into_response();
                    }
                }
            }
            _ => {
                // ignore
            }
        }
    }

    if chunk_saved {
        log::info!("[/upload_chunk] saved {:?}", chunk_path_saved);
        let index = info.as_ref().and_then(|i| i.index).unwrap_or(0);
        (StatusCode::CREATED, format!("chunk_{index}")).into_response()
    } else {
        (StatusCode::BAD_REQUEST, "chunk upload failed").into_response()
    }
}

#[allow(dead_code)]
pub fn body_limit() -> DefaultBodyLimit {
    DefaultBodyLimit::max(64 * 1024 * 1024 * 1024)
}
