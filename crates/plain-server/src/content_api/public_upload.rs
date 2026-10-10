//! `POST /upload` and `POST /upload_chunk` for the public web UI.
//!
//! The `info` part is base64 of the XChaCha20-Poly1305 blob encrypted with the
//! session token that Rust itself issued and stored in the `sessions` row, so
//! no host round trip is needed to authenticate a part.
//!
//! Behaviour mirrors plain-app's `UploadRoutes.kt`: file bodies stream to a
//! temp file, the received size is verified against `info.size`, and the final
//! placement is an atomic rename so a cancelled upload never leaves a partial
//! file at the destination.

use super::server::ServerState;
use axum::{
    extract::{FromRequest, Multipart, Request, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct UploadInfo {
    dir: String,
    replace: bool,
    is_app_file: bool,
    size: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ChunkInfo {
    file_id: String,
    index: i64,
    size: i64,
}

/// plain-app's `File.newName()`: an existing ` (N)` suffix is stripped before
/// the next index is appended, so repeated uploads walk `a.txt`, `a (1).txt`,
/// `a (2).txt` rather than nesting counters.
fn split_name(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(position) if position > 0 => (&name[..position], &name[position + 1..]),
        _ => (name, ""),
    }
}

fn candidate(stem: &str, extension: &str, index: u32) -> String {
    if extension.is_empty() {
        format!("{stem} ({index})")
    } else {
        format!("{stem} ({index}).{extension}")
    }
}

fn next_unique_name(parent: &Path, name: &str) -> String {
    let (stem, extension) = split_name(name);
    // Drop a trailing ` (N)` exactly like plain-app before appending the next.
    let stem = {
        let mut base = stem;
        if let Some(position) = base.rfind(" (") {
            let suffix = &base[position + 2..];
            if !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit()) {
                base = &base[..position];
            }
        }
        base
    };
    let mut index = 1u32;
    loop {
        let name = candidate(stem, extension, index);
        if !parent.join(&name).exists() {
            return name;
        }
        index += 1;
        if index == u32::MAX {
            return name;
        }
    }
}

fn plain(status: StatusCode, body: impl Into<String>) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        body.into(),
    )
        .into_response()
}

fn desktop_access_allowed(state: &ServerState) -> bool {
    state.prefs.get_user_or("desktop_access", true)
}

/// The session token Rust issued, straight from the `sessions` row.
async fn session_key(state: &ServerState, headers: &HeaderMap) -> anyhow::Result<Vec<u8>> {
    let client_id = headers
        .get("c-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    anyhow::ensure!(!client_id.is_empty(), "c-id header is missing");
    let row = state
        .db
        .session_get(client_id)
        .map_err(|error| anyhow::Error::msg(error.to_string()))?
        .ok_or_else(|| anyhow::anyhow!("unknown client"))?;
    let key = crate::utils::base64::base64_decode(&row.token);
    anyhow::ensure!(key.len() == 32, "session token is not a 32-byte key");
    Ok(key)
}

async fn read_info<T: for<'de> Deserialize<'de>>(
    key: &[u8],
    field: axum::extract::multipart::Field<'_>,
) -> anyhow::Result<T> {
    let bytes = field
        .bytes()
        .await
        .map_err(|error| anyhow::Error::msg(error.to_string()))?;
    let plaintext = crate::crypto::xchacha_decrypt_raw(key, &bytes)
        .ok_or_else(|| anyhow::anyhow!("Unauthorized"))?;
    Ok(serde_json::from_slice(&plaintext)?)
}

/// Streams one multipart part into `destination`, returning the byte count.
async fn stream_to_file(
    field: &mut axum::extract::multipart::Field<'_>,
    destination: &Path,
) -> anyhow::Result<u64> {
    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| anyhow::Error::msg(error.to_string()))?;
    }
    let mut file = tokio::fs::File::create(destination)
        .await
        .map_err(|error| anyhow::Error::msg(error.to_string()))?;
    let mut written = 0u64;
    loop {
        let chunk = field
            .chunk()
            .await
            .map_err(|error| anyhow::Error::msg(error.to_string()))?;
        let Some(chunk) = chunk else { break };
        file.write_all(&chunk)
            .await
            .map_err(|error| anyhow::Error::msg(error.to_string()))?;
        written += chunk.len() as u64;
    }
    file.flush()
        .await
        .map_err(|error| anyhow::Error::msg(error.to_string()))?;
    Ok(written)
}

fn temp_name(prefix: &str, millis: u128) -> String {
    format!(".{prefix}_{millis}_{}", std::process::id())
}

fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_millis())
        .unwrap_or_default()
}

pub(super) async fn upload_tmp_dir(state: &ServerState) -> anyhow::Result<String> {
    let facts = state
        .host
        .call("uploadTmpDirFacts", json!({}))
        .await
        .map_err(anyhow::Error::msg)?;
    facts["path"]
        .as_str()
        .map(str::to_owned)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| anyhow::anyhow!("upload temp directory is unavailable"))
}

pub(super) async fn upload(State(state): State<ServerState>, request: Request) -> Response {
    if !desktop_access_allowed(&state) {
        return plain(StatusCode::FORBIDDEN, "forbidden");
    }
    let key = match session_key(&state, request.headers()).await {
        Ok(key) => key,
        Err(_) => return plain(StatusCode::UNAUTHORIZED, "unauthorized"),
    };
    let mut multipart = match Multipart::from_request(request, &()).await {
        Ok(multipart) => multipart,
        Err(_) => return plain(StatusCode::BAD_REQUEST, "invalid multipart request"),
    };
    let mut info: Option<UploadInfo> = None;
    let mut saved: Option<String> = None;
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(_) => return plain(StatusCode::BAD_REQUEST, "invalid multipart body"),
        };
        let name = field.name().unwrap_or_default().to_owned();
        let mut field = field;
        match name.as_str() {
            "info" => match read_info(&key, field).await {
                Ok(value) => info = Some(value),
                Err(error) if error.to_string() == "Unauthorized" => {
                    return plain(StatusCode::UNAUTHORIZED, "unauthorized");
                }
                Err(error) => return plain(StatusCode::BAD_REQUEST, error.to_string()),
            },
            "file" => {
                let Some(current) = info.clone() else {
                    return plain(StatusCode::BAD_REQUEST, "info part must precede file part");
                };
                let file_name = field
                    .file_name()
                    .unwrap_or_default()
                    .rsplit(['/', '\\'])
                    .next()
                    .unwrap_or_default()
                    .to_owned();
                if file_name.is_empty() || current.dir.is_empty() {
                    return plain(StatusCode::BAD_REQUEST, "dir or fileName is empty");
                }
                let content_type = field.content_type().unwrap_or_default().to_owned();
                let destination_dir = PathBuf::from(&current.dir);
                if current.is_app_file {
                    match store_app_file(
                        &state,
                        &mut field,
                        &file_name,
                        &content_type,
                        current.size,
                    )
                    .await
                    {
                        Ok(suffix) => saved = Some(suffix),
                        Err(error) => return plain(StatusCode::BAD_REQUEST, error.to_string()),
                    }
                    continue;
                }
                let mut final_name = file_name.clone();
                let mut destination = destination_dir.join(&file_name);
                if destination.is_file() {
                    if current.replace {
                        let _ = tokio::fs::remove_file(&destination).await;
                    } else {
                        final_name = next_unique_name(&destination_dir, &file_name);
                        destination = destination_dir.join(&final_name);
                    }
                }
                let temp = destination_dir.join(temp_name("upload_tmp", now_millis()));
                let written = match stream_to_file(&mut field, &temp).await {
                    Ok(written) => written,
                    Err(error) => {
                        let _ = tokio::fs::remove_file(&temp).await;
                        return plain(StatusCode::BAD_REQUEST, error.to_string());
                    }
                };
                if current.size > 0 && written != current.size as u64 {
                    let _ = tokio::fs::remove_file(&temp).await;
                    return plain(
                        StatusCode::BAD_REQUEST,
                        format!("Size mismatch: expected {}, got {written}", current.size),
                    );
                }
                if let Err(error) = tokio::fs::rename(&temp, &destination).await {
                    let _ = tokio::fs::remove_file(&temp).await;
                    return plain(
                        StatusCode::BAD_REQUEST,
                        format!("Failed to move uploaded file into place: {error}"),
                    );
                }
                let _ = state
                    .host
                    .call(
                        "scanFilesFacts",
                        json!({"paths":[destination.to_string_lossy()]}),
                    )
                    .await;
                saved = Some(final_name);
            }
            _ => {
                let _ = field.bytes().await;
            }
        }
    }
    match saved {
        Some(name) => plain(StatusCode::CREATED, name),
        None => plain(StatusCode::BAD_REQUEST, "no file uploaded"),
    }
}

async fn store_app_file(
    state: &ServerState,
    field: &mut axum::extract::multipart::Field<'_>,
    name: &str,
    content_type: &str,
    expected: i64,
) -> anyhow::Result<String> {
    let directory = state.directory.clone();
    let temp = directory.join(temp_name("chat_upload", now_millis()));
    let written = match stream_to_file(field, &temp).await {
        Ok(written) => written,
        Err(error) => {
            let _ = tokio::fs::remove_file(&temp).await;
            return Err(error);
        }
    };
    if expected > 0 && written != expected as u64 {
        let _ = tokio::fs::remove_file(&temp).await;
        return Err(anyhow::anyhow!(
            "Size mismatch: expected {expected}, got {written}"
        ));
    }
    let store = crate::app_files::FileStore::new(state.db.clone(), directory);
    match store
        .import(temp.clone(), name.to_owned(), content_type.to_owned(), true)
        .await
    {
        // The client keys the stored file by `{hash}.{ext}`, so the suffix must
        // come from the record's real path (a dedup hit can carry a different
        // extension than a fresh guess would produce).
        Ok(record) => Path::new(&record.real_path)
            .file_name()
            .and_then(|value| value.to_str())
            .map(str::to_owned)
            .ok_or_else(|| anyhow::anyhow!("Failed to import app file")),
        Err(error) => {
            let _ = tokio::fs::remove_file(&temp).await;
            Err(anyhow::anyhow!(error))
        }
    }
}

pub(super) async fn upload_chunk(State(state): State<ServerState>, request: Request) -> Response {
    if !desktop_access_allowed(&state) {
        return plain(StatusCode::FORBIDDEN, "forbidden");
    }
    let key = match session_key(&state, request.headers()).await {
        Ok(key) => key,
        Err(_) => return plain(StatusCode::UNAUTHORIZED, "unauthorized"),
    };
    let mut multipart = match Multipart::from_request(request, &()).await {
        Ok(multipart) => multipart,
        Err(_) => return plain(StatusCode::BAD_REQUEST, "invalid multipart request"),
    };
    let mut info: Option<ChunkInfo> = None;
    let mut saved_size = 0i64;
    let mut failure: Option<Response> = None;
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(_) => {
                failure = Some(plain(StatusCode::BAD_REQUEST, "invalid multipart body"));
                break;
            }
        };
        let name = field.name().unwrap_or_default().to_owned();
        let mut field = field;
        match name.as_str() {
            "info" => match read_info(&key, field).await {
                Ok(value) => info = Some(value),
                Err(error) if error.to_string() == "Unauthorized" => {
                    failure = Some(plain(StatusCode::UNAUTHORIZED, "unauthorized"));
                    break;
                }
                Err(error) => {
                    failure = Some(plain(StatusCode::BAD_REQUEST, error.to_string()));
                    break;
                }
            },
            "file" => {
                let Some(current) = info.clone() else {
                    failure = Some(plain(
                        StatusCode::BAD_REQUEST,
                        "info part must precede file part",
                    ));
                    break;
                };
                if current.file_id.is_empty() || current.index < 0 {
                    failure = Some(plain(
                        StatusCode::BAD_REQUEST,
                        "fileId or index is missing or invalid",
                    ));
                    break;
                }
                let base = match upload_tmp_dir(&state).await {
                    Ok(base) => match crate::uploads::chunk_path(
                        Path::new(&base),
                        &current.file_id,
                        current.index,
                    ) {
                        Ok(path) => path,
                        Err(error) => {
                            failure = Some(plain(StatusCode::BAD_REQUEST, error.to_string()));
                            break;
                        }
                    },
                    Err(error) => {
                        failure = Some(plain(StatusCode::BAD_REQUEST, error.to_string()));
                        break;
                    }
                };
                let chunk = base;
                let temp = chunk.parent().unwrap().join(temp_name(
                    &format!("tmp_chunk_{}_{}", current.index, now_millis()),
                    std::process::id() as u128,
                ));
                match stream_to_file(&mut field, &temp).await {
                    Ok(written) => {
                        if current.size > 0 && written != current.size as u64 {
                            let _ = tokio::fs::remove_file(&temp).await;
                            failure = Some(plain(
                                StatusCode::BAD_REQUEST,
                                format!(
                                    "Chunk {} size mismatch: expected {}, received {written}",
                                    current.index, current.size
                                ),
                            ));
                            break;
                        }
                        let _ = tokio::fs::remove_file(&chunk).await;
                        if tokio::fs::rename(&temp, &chunk).await.is_err() {
                            // Some Android filesystems report a failed rename
                            // that still moved the file; trust the size of the
                            // final path, exactly like the Kotlin route.
                            let final_size = tokio::fs::metadata(&chunk)
                                .await
                                .map(|meta| meta.len() as i64)
                                .unwrap_or(-1);
                            if final_size < 0 {
                                let _ = tokio::fs::remove_file(&temp).await;
                                failure = Some(plain(
                                    StatusCode::BAD_REQUEST,
                                    format!(
                                        "Failed to save chunk {}: rename failed and source file is missing",
                                        current.index
                                    ),
                                ));
                                break;
                            }
                            saved_size = final_size;
                        } else {
                            saved_size = match tokio::fs::metadata(&chunk).await {
                                Ok(meta) => meta.len() as i64,
                                Err(_) => 0,
                            };
                        }
                        if current.size > 0 && saved_size != current.size {
                            let _ = tokio::fs::remove_file(&chunk).await;
                            failure = Some(plain(
                                StatusCode::BAD_REQUEST,
                                format!(
                                    "Chunk {} final size mismatch: expected {}, saved {saved_size}",
                                    current.index, current.size
                                ),
                            ));
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = tokio::fs::remove_file(&temp).await;
                        failure = Some(plain(StatusCode::BAD_REQUEST, error.to_string()));
                        break;
                    }
                }
            }
            _ => {
                let _ = field.bytes().await;
            }
        }
    }
    if let Some(response) = failure {
        return response;
    }
    let index = info.map(|value| value.index).unwrap_or(0);
    if saved_size > 0 {
        plain(StatusCode::CREATED, format!("{index}:{saved_size}"))
    } else {
        plain(StatusCode::BAD_REQUEST, "chunk upload failed")
    }
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/public_upload.rs"]
mod tests;
