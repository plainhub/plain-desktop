use super::server::ServerState;
use crate::utils::keyed_locks::KeyedLocks;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Mutex,
};
use tokio::sync::{Semaphore, oneshot};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Request {
    pub(super) path: String,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) center_crop: bool,
    pub(super) media_id: String,
    pub(super) file_name: String,
    #[serde(default)]
    pub(super) if_none_match: Option<String>,
}
pub(super) struct Thumbnails {
    locks: KeyedLocks,
    capacity: Semaphore,
    outputs: Mutex<HashMap<String, oneshot::Sender<Vec<u8>>>>,
}
impl Default for Thumbnails {
    fn default() -> Self {
        Self {
            locks: KeyedLocks::new(),
            capacity: Semaphore::new(4),
            outputs: Mutex::new(HashMap::new()),
        }
    }
}
async fn fingerprint(request: &Request) -> Result<Option<String>, String> {
    let metadata = match tokio::fs::metadata(&request.path).await {
        Ok(v) if v.is_file() => v,
        Ok(_) => return Ok(None),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let modified = metadata.modified().map_err(|e| e.to_string())?;
    let input = json!([
        request.path,
        metadata.len(),
        format!("{modified:?}"),
        request.width,
        request.height,
        request.center_crop,
        request.media_id,
        request.file_name,
        "mobile-v1"
    ])
    .to_string();
    Ok(Some(crate::utils::hex::bytes_to_hex(&Sha256::digest(
        input.as_bytes(),
    ))))
}
async fn decode(
    state: &ServerState,
    request: &Request,
    system: bool,
) -> Result<Option<Vec<u8>>, String> {
    let token = uuid::Uuid::new_v4().to_string();
    let (sender, mut receiver) = oneshot::channel();
    state
        .thumbnails
        .outputs
        .lock()
        .unwrap()
        .insert(token.clone(), sender);
    let _output = Output {
        service: &state.thumbnails,
        token: token.clone(),
    };
    let result=state.host.call_wait("thumbnailDecode",json!({"outputToken":token,"path":request.path,"width":request.width,"height":request.height,"centerCrop":request.center_crop,"mediaId":if system { &request.media_id }else{""},"fileName":request.file_name})).await?;
    if result.is_null() {
        return Ok(None);
    }
    if result.as_bool() != Some(true) {
        return Err("invalid thumbnail receipt".into());
    }
    let bytes = receiver
        .try_recv()
        .map_err(|_| "missing thumbnail output")?;
    if bytes.is_empty() {
        return Err("empty thumbnail output".into());
    }
    Ok(Some(bytes))
}
struct Output<'a> {
    service: &'a Thumbnails,
    token: String,
}
impl Drop for Output<'_> {
    fn drop(&mut self) {
        self.service.outputs.lock().unwrap().remove(&self.token);
    }
}
pub(super) async fn output(
    State(state): State<ServerState>,
    axum::extract::Path(token): axum::extract::Path<String>,
    headers: HeaderMap,
    body: axum::body::Body,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let sender = state.thumbnails.outputs.lock().unwrap().remove(&token);
    let Some(sender) = sender else {
        return StatusCode::GONE.into_response();
    };
    match axum::body::to_bytes(body, usize::MAX).await {
        Ok(bytes) if !bytes.is_empty() => {
            if sender.send(bytes.to_vec()).is_ok() {
                StatusCode::NO_CONTENT.into_response()
            } else {
                StatusCode::GONE.into_response()
            }
        }
        _ => StatusCode::BAD_REQUEST.into_response(),
    }
}

impl Thumbnails {
    async fn get(
        &self,
        state: &ServerState,
        mut request: Request,
    ) -> Result<Option<Vec<u8>>, String> {
        if request.width == 0 || request.height == 0 || !Path::new(&request.path).is_absolute() {
            return Err("positive thumbnail dimensions and absolute path required".into());
        }
        let canonical = match tokio::fs::canonicalize(&request.path).await {
            Ok(p) => p,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.to_string()),
        };
        request.path = canonical.to_string_lossy().into_owned();
        let cache = state
            .host
            .call("thumbnailAuthorize", json!({"paths":[request.path]}))
            .await?;
        let cache = PathBuf::from(cache.as_str().ok_or("invalid thumbnail cache root")?);
        if !cache.is_absolute() {
            return Err("absolute thumbnail cache root required".into());
        }
        let Some(key) = fingerprint(&request).await? else {
            return Ok(None);
        };
        let cached = cache
            .join("thumbs")
            .join("rust")
            .join(format!("{key}.thumb"));
        let system = !request.media_id.is_empty() && request.width <= 512;
        self.locks
            .with_lock(key.clone(), async {
                if !system {
                    match tokio::fs::read(&cached).await {
                        Ok(bytes) if !bytes.is_empty() => return Ok(Some(bytes)),
                        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                            return Err(e.to_string());
                        }
                        _ => {}
                    }
                }
                let _permit = self.capacity.acquire().await.map_err(|e| e.to_string())?;
                if system {
                    if let Some(bytes) = decode(state, &request, true).await? {
                        return Ok(if fingerprint(&request).await?.as_deref() == Some(&key) {
                            Some(bytes)
                        } else {
                            None
                        });
                    }
                }
                match tokio::fs::read(&cached).await {
                    Ok(bytes) if !bytes.is_empty() => return Ok(Some(bytes)),
                    Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.to_string()),
                    _ => {}
                }
                let Some(bytes) = decode(state, &request, false).await? else {
                    return Ok(None);
                };
                if fingerprint(&request).await?.as_deref() != Some(&key) {
                    return Ok(None);
                }
                let stored = bytes.clone();
                tokio::task::spawn_blocking(move || -> std::io::Result<()> {
                    use std::io::Write;
                    std::fs::create_dir_all(cached.parent().unwrap())?;
                    let temporary = cached.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
                    let _cleanup = Temporary(temporary.clone());
                    let mut file = std::fs::OpenOptions::new()
                        .create_new(true)
                        .write(true)
                        .open(&temporary)?;
                    file.write_all(&stored)?;
                    file.flush()?;
                    drop(file);
                    std::fs::rename(&temporary, &cached)
                })
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())?;
                Ok(Some(bytes))
            })
            .await
    }
}
struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
pub(super) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    response(&state, request).await
}
pub(super) async fn response(state: &ServerState, request: Request) -> Response {
    let conditional = request.if_none_match.clone();
    match state.thumbnails.get(state, request).await {
        Ok(Some(bytes)) => {
            let mime = if bytes.starts_with(b"\x89PNG") {
                "image/png"
            } else {
                "image/jpeg"
            };
            let etag = format!(
                "\"{}\"",
                crate::utils::hex::bytes_to_hex(&Sha256::digest(&bytes))
            );
            if conditional.as_deref().is_some_and(|value| {
                value.split(',').any(|tag| {
                    let tag = tag.trim();
                    tag == "*" || tag.strip_prefix("W/").unwrap_or(tag) == etag
                })
            }) {
                return (
                    StatusCode::NOT_MODIFIED,
                    [
                        ("etag", etag),
                        ("cache-control", "private, max-age=86400".to_string()),
                    ],
                )
                    .into_response();
            }
            (
                [
                    ("content-type", mime.to_string()),
                    ("etag", etag),
                    ("cache-control", "private, max-age=86400".to_string()),
                ],
                bytes,
            )
                .into_response()
        }
        Ok(None) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, Json(json!({"error":error}))).into_response(),
    }
}
#[cfg(test)]
#[path = "../../tests/unit/content_api/thumbnails.rs"]
mod tests;
