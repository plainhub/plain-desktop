//! HTTP upload endpoints for local mode chat file uploads.
//!
//! Mirrors `plain-app/.../web/routes/Upload.kt`:
//!
//! - `POST /upload`        — direct (small-file) upload, multipart with an
//!   `info` part (XChaCha20-encrypted JSON) and a `file` part.
//! - `POST /upload_chunk`  — single chunk of a larger file, same multipart
//!   shape but `info` carries `{fileId, index, size}`.
//!
//! Both endpoints authenticate via the `c-id` header (checked before the
//! multipart body is consumed). The `info` part is encrypted with the
//! local server's URL token (`ctx.token`); the server decrypts it with
//! the same token, matching how `Upload.kt` authenticates against
//! `HttpServerManager.tokenCache[clientId]`.
//!
//! Chat uploads are bounded to a 5 MB chunk size (the web client's
//! `CHUNK_SIZE` in `lib/upload/upload.ts`); the route-level
//! `DefaultBodyLimit` enforces the safety cap that used to be a manual
//! content-length check.

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{FromRequest, Request, State};
use axum::response::Response;

use super::response::respond;
use crate::chat::app_file_store;
use crate::api::context::AppCtx;
use crate::api::server::ServerState;

/// 16 MB safety cap applied per upload route via `DefaultBodyLimit`; the
/// client should never send more than ~5 MB per request.
pub const MAX_BODY_BYTES: usize = 16 * 1024 * 1024;

pub async fn upload_handler(State(state): State<ServerState>, req: Request) -> Response {
    let Some(client_id) = req
        .headers()
        .get("c-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
    else {
        return respond(401, Vec::new(), "text/plain");
    };
    if client_id != state.ctx.identity.client_id {
        return respond(401, Vec::new(), "text/plain");
    }
    let mut multipart = match axum::extract::Multipart::from_request(req, &()).await {
        Ok(m) => m,
        Err(_) => return respond(400, b"invalid multipart request".to_vec(), "text/plain"),
    };

    let mut info: Option<Vec<u8>> = None;
    let mut file: Option<UploadFilePart> = None;
    loop {
        let field = match multipart.next_field().await {
            Ok(f) => f,
            Err(_) => return respond(400, b"invalid multipart body".to_vec(), "text/plain"),
        };
        let Some(field) = field else { break };
        let name = field.name().map(|s| s.to_owned());
        match name.as_deref() {
            Some("info") => {
                let bytes = match field.bytes().await {
                    Ok(b) => b,
                    Err(_) => {
                        return respond(400, b"invalid multipart body".to_vec(), "text/plain");
                    }
                };
                info = Some(bytes.to_vec());
            }
            Some("file") => {
                let filename = field.file_name().map(|s| s.to_owned());
                let content_type = field.content_type().map(|s| s.to_owned());
                let bytes = match field.bytes().await {
                    Ok(b) => b,
                    Err(_) => {
                        return respond(400, b"invalid multipart body".to_vec(), "text/plain");
                    }
                };
                file = Some(UploadFilePart {
                    filename,
                    content_type,
                    body: bytes.to_vec(),
                });
            }
            _ => {
                // Drain unknown fields so the parser stays in sync.
                let _ = field.bytes().await;
            }
        }
    }

    let Some(info_bytes) = info else {
        return respond(400, b"missing info part".to_vec(), "text/plain");
    };
    let Some(file_part) = file else {
        return respond(400, b"missing file part".to_vec(), "text/plain");
    };

    handle_upload(&state.ctx, &info_bytes, &file_part).await
}

async fn handle_upload(
    ctx: &Arc<AppCtx>,
    info_bytes: &[u8],
    file_part: &UploadFilePart,
) -> Response {
    // Decrypt info JSON.
    let Some(plaintext) = crate::xchacha_decrypt(&ctx.token, info_bytes) else {
        return respond(401, Vec::new(), "text/plain");
    };
    let info: serde_json::Value = match serde_json::from_slice(&plaintext) {
        Ok(v) => v,
        Err(e) => {
            return respond(
                400,
                format!("bad info json: {e}").into_bytes(),
                "text/plain",
            );
        }
    };
    let is_app_file = info
        .get("isAppFile")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let info_dir = info
        .get("dir")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let info_size = info.get("size").and_then(|v| v.as_i64()).unwrap_or(0);

    if info_size > 0 && file_part.body.len() as i64 != info_size {
        let msg = format!(
            "Size mismatch: expected {info_size}, got {}",
            file_part.body.len()
        );
        return respond(400, msg.into_bytes(), "text/plain");
    }

    // Stage the file body to a temp file (in case `isAppFile` triggers the
    // hash + dedup pipeline, which needs a real on-disk file).
    let temp = match stage_to_temp(ctx, &file_part.body).await {
        Ok(p) => p,
        Err(e) => return respond(500, e.into_bytes(), "text/plain"),
    };

    if is_app_file {
        let file_name = file_part.filename.clone().unwrap_or_default();
        match app_file_store::import_file(
            &ctx.db,
            &ctx.data_dir,
            &temp,
            &file_name,
            file_part.content_type.as_deref().unwrap_or_default(),
        ) {
            Ok(result) => {
                let _ = tokio::fs::remove_file(&temp).await;
                respond(201, result.fid_suffix.into_bytes(), "text/plain")
            }
            Err(e) => {
                let _ = tokio::fs::remove_file(&temp).await;
                respond(500, e.to_string().into_bytes(), "text/plain")
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
        match tokio::fs::write(&target, &file_part.body).await {
            Ok(_) => {
                let _ = tokio::fs::remove_file(&temp).await;
                respond(201, safe_name.into_bytes(), "text/plain")
            }
            Err(e) => {
                let _ = tokio::fs::remove_file(&temp).await;
                respond(500, e.to_string().into_bytes(), "text/plain")
            }
        }
    }
}

pub async fn upload_chunk_handler(State(state): State<ServerState>, req: Request) -> Response {
    let Some(client_id) = req
        .headers()
        .get("c-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
    else {
        return respond(401, Vec::new(), "text/plain");
    };
    if client_id != state.ctx.identity.client_id {
        return respond(401, Vec::new(), "text/plain");
    }
    let mut multipart = match axum::extract::Multipart::from_request(req, &()).await {
        Ok(m) => m,
        Err(_) => return respond(400, b"invalid multipart request".to_vec(), "text/plain"),
    };

    let mut info: Option<Vec<u8>> = None;
    let mut file: Option<Vec<u8>> = None;
    loop {
        let field = match multipart.next_field().await {
            Ok(f) => f,
            Err(_) => return respond(400, b"invalid multipart body".to_vec(), "text/plain"),
        };
        let Some(field) = field else { break };
        let name = field.name().map(|s| s.to_owned());
        match name.as_deref() {
            Some("info") => {
                let bytes = match field.bytes().await {
                    Ok(b) => b,
                    Err(_) => {
                        return respond(400, b"invalid multipart body".to_vec(), "text/plain");
                    }
                };
                info = Some(bytes.to_vec());
            }
            Some("file") => {
                let bytes = match field.bytes().await {
                    Ok(b) => b,
                    Err(_) => {
                        return respond(400, b"invalid multipart body".to_vec(), "text/plain");
                    }
                };
                file = Some(bytes.to_vec());
            }
            _ => {
                let _ = field.bytes().await;
            }
        }
    }

    let Some(info_bytes) = info else {
        return respond(400, b"missing info part".to_vec(), "text/plain");
    };
    let Some(file_body) = file else {
        return respond(400, b"missing file part".to_vec(), "text/plain");
    };

    handle_upload_chunk(&state.ctx, &info_bytes, &file_body).await
}

async fn handle_upload_chunk(ctx: &Arc<AppCtx>, info_bytes: &[u8], file_body: &[u8]) -> Response {
    let Some(plaintext) = crate::xchacha_decrypt(&ctx.token, info_bytes) else {
        return respond(401, Vec::new(), "text/plain");
    };
    let info: serde_json::Value = match serde_json::from_slice(&plaintext) {
        Ok(v) => v,
        Err(e) => {
            return respond(
                400,
                format!("bad info json: {e}").into_bytes(),
                "text/plain",
            );
        }
    };
    let file_id = info
        .get("fileId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let index = info.get("index").and_then(|v| v.as_i64()).unwrap_or(-1) as i32;
    let expected_size = info.get("size").and_then(|v| v.as_i64()).unwrap_or(0);

    if file_id.is_empty() || index < 0 {
        return respond(
            400,
            b"fileId or index is missing or invalid".to_vec(),
            "text/plain",
        );
    }

    let saved_size = file_body.len() as u64;
    if expected_size > 0 && saved_size as i64 != expected_size {
        let msg =
            format!("Chunk {index} size mismatch: expected {expected_size}, received {saved_size}");
        return respond(400, msg.into_bytes(), "text/plain");
    }

    let dir = ctx.data_dir.join("upload_tmp").join(&file_id);
    if let Err(e) = tokio::fs::create_dir_all(&dir).await {
        let msg = format!("create_dir_all failed: {e}");
        return respond(500, msg.into_bytes(), "text/plain");
    }
    let chunk_path = dir.join(format!("chunk_{index}"));
    if let Err(e) = tokio::fs::write(&chunk_path, file_body).await {
        let msg = format!("chunk write failed: {e}");
        return respond(500, msg.into_bytes(), "text/plain");
    }
    let final_size = tokio::fs::metadata(&chunk_path)
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    if expected_size > 0 && final_size as i64 != expected_size {
        let _ = tokio::fs::remove_file(&chunk_path).await;
        let msg = format!(
            "Chunk {index} final size mismatch: expected {expected_size}, saved {final_size}"
        );
        return respond(400, msg.into_bytes(), "text/plain");
    }

    let body = format!("{index}:{final_size}");
    respond(201, body.into_bytes(), "text/plain")
}

struct UploadFilePart {
    filename: Option<String>,
    content_type: Option<String>,
    body: Vec<u8>,
}

async fn stage_to_temp(ctx: &Arc<AppCtx>, data: &[u8]) -> Result<PathBuf, String> {
    let dir = ctx.data_dir.join("upload_tmp");
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| e.to_string())?;
    let path = dir.join(format!("upload_{}_{}.bin", std::process::id(), now_ms()));
    tokio::fs::write(&path, data)
        .await
        .map_err(|e| e.to_string())?;
    Ok(path)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
