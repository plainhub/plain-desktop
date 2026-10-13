//! `GET /zip/dir` and `GET /zip/files` — archive downloads for the web UI.
//!
//! Three directory shapes are supported, matching plain-app's `ZipRoutes.kt`:
//! a share link (`?sid=`, encrypted with the share's own url token), a virtual
//! path inside an existing archive (`….zip!zip!/inner/path`), and a real
//! directory. `/zip/files` zips a search result instead.

use crate::assembly::server::ServerState;
use axum::{
    body::Body,
    extract::{Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};

const ZIP_SEPARATOR: &str = "!zip!/";
const MAX_ENTRIES: usize = 100_000;

static NEXT_STAGING: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[derive(Deserialize)]
pub(crate) struct Params {
    #[serde(default)]
    id: String,
    #[serde(default)]
    sid: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct ZipFilesRequest {
    r#type: String,
    query: String,
    id: String,
    name: String,
}

fn desktop_access_allowed(state: &ServerState) -> bool {
    state.prefs.get_user_or("desktop_access", true)
}

fn plain(status: StatusCode, body: impl Into<String>) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        body.into(),
    )
        .into_response()
}

/// plain-app percent-encodes the download name; the desktop route does the
/// same before writing the `Content-Disposition`.
fn url_escape(value: &str) -> String {
    crate::host::filesystem::escape(value)
}

/// Decrypt a urlToken-protected `id` into its plaintext payload.
fn decrypt_url_id(state: &ServerState, id: &str) -> Option<String> {
    let token = state
        .prefs
        .get::<String>(plain_rs::prefs::system::keys::URL_TOKEN)
        .ok()
        .flatten()
        .unwrap_or_default();
    let key = plain_rs::utils::base64::base64_decode(&token);
    if key.len() != 32 {
        return None;
    }
    let plaintext =
        plain_rs::crypto::xchacha_decrypt_raw(&key, &plain_rs::utils::base64::base64_decode(id))?;
    String::from_utf8(plaintext).ok()
}

/// `….zip!zip!/inner` → (archive, inner prefix).
fn split_zip_path(path: &str) -> Option<(PathBuf, String)> {
    let position = path.find(ZIP_SEPARATOR)?;
    let archive = PathBuf::from(&path[..position]);
    let inner = path[position + ZIP_SEPARATOR.len()..].trim_matches('/');
    if !archive.is_file() {
        return None;
    }
    Some((archive, inner.to_owned()))
}

fn walk_directory(
    root: &Path,
    prefix: &str,
    out: &mut Vec<(PathBuf, String)>,
) -> anyhow::Result<()> {
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory)? {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            let relative = path
                .strip_prefix(root)
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_else(|_| name.clone());
            let relative = relative.replace('\\', "/");
            let entry_name = if prefix.is_empty() {
                relative
            } else {
                format!("{prefix}/{relative}")
            };
            if path.is_dir() {
                pending.push(path);
            } else if out.len() < MAX_ENTRIES {
                out.push((path, entry_name));
            }
        }
    }
    Ok(())
}

/// Copy the entries under `inner` out of an open archive.
fn collect_archive_entries(
    archive: &Path,
    inner: &str,
    staging: &Staging,
    out: &mut Vec<(PathBuf, String)>,
) -> anyhow::Result<()> {
    let file = std::fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file)?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_owned();
        let relative = if inner.is_empty() {
            name.clone()
        } else if let Some(rest) = name.strip_prefix(&format!("{inner}/")) {
            rest.to_owned()
        } else {
            continue;
        };
        if relative.is_empty() || relative.contains("..") || out.len() >= MAX_ENTRIES {
            continue;
        }
        let mut target = std::fs::File::create(staging.entry_path(out.len()))?;
        let mut buffer = vec![0; 64 * 1024];
        loop {
            let read = entry.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            target.write_all(&buffer[..read])?;
        }
        out.push((staging.entry_path(out.len()), relative));
    }
    Ok(())
}

/// Extracted entries of one request, deleted with the directory when it ends.
struct Staging {
    directory: PathBuf,
}

impl Staging {
    fn create() -> std::io::Result<Self> {
        let sequence = NEXT_STAGING.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let directory =
            std::env::temp_dir().join(format!("plain-zip-{}-{sequence}", std::process::id()));
        std::fs::create_dir_all(&directory)?;
        Ok(Self { directory })
    }

    fn entry_path(&self, index: usize) -> PathBuf {
        self.directory.join(format!("{index}.entry"))
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

async fn stream_archive(
    state: &ServerState,
    items: Vec<(PathBuf, String)>,
    name: &str,
    staging: Option<Staging>,
    permit: tokio::sync::OwnedSemaphorePermit,
) -> Response {
    let _staging = staging;
    let reader = match state
        .archives
        .build(items, permit, state.stop.clone())
        .await
    {
        Ok(reader) => reader,
        Err(error) => return plain(StatusCode::BAD_REQUEST, error),
    };
    let encoded = url_escape(name);
    let disposition = format!("attachment;filename=\"{encoded}\";filename*=utf-8''{encoded}");
    let length = reader.size;
    let stream = futures_util::stream::try_unfold(reader, |mut reader| async move {
        reader
            .read()
            .await
            .map(|chunk| chunk.map(|bytes| (axum::body::Bytes::from(bytes), reader)))
            .map_err(std::io::Error::other)
    });
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/zip")
        .header(header::CONTENT_DISPOSITION, disposition)
        .header(header::CONTENT_LENGTH, length)
        .body(Body::from_stream(stream))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}
async fn zip_dir(
    state: &ServerState,
    params: &Params,
    permit: tokio::sync::OwnedSemaphorePermit,
) -> Response {
    if params.id.is_empty() {
        return plain(StatusCode::BAD_REQUEST, "");
    }
    // Share links stay reachable without desktop access, exactly like before.
    if let Some(sid) = params.sid.as_deref() {
        let shares = plain_rs::files::shares::Service::new(state.db.clone(), state.prefs.clone());
        let path = match shares.resolve_file(sid, &params.id) {
            Ok(Some(path)) => PathBuf::from(path),
            _ => return plain(StatusCode::FORBIDDEN, ""),
        };
        let entries = match shares.archive(sid, &params.id) {
            Ok(entries) => entries,
            Err(error) => return plain(StatusCode::BAD_REQUEST, error.to_string()),
        };
        let items: Vec<(PathBuf, String)> = entries
            .into_iter()
            .map(|entry| (PathBuf::from(entry.real_path), entry.virtual_path))
            .filter(|(path, _)| path.is_file())
            .collect();
        let name = format!(
            "{}.zip",
            path.file_name()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_default()
        );
        return stream_archive(state, items, &name, None, permit).await;
    }
    if !desktop_access_allowed(state) {
        return plain(StatusCode::FORBIDDEN, "");
    }
    let Some(plaintext) = decrypt_url_id(state, &params.id) else {
        return plain(StatusCode::FORBIDDEN, "File is expired or does not exist.");
    };
    let (path, json_name) = if plaintext.starts_with('{') {
        let value: Value = serde_json::from_str(&plaintext).unwrap_or(Value::Null);
        (
            value
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            value
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        )
    } else {
        (plaintext.clone(), String::new())
    };
    if path.is_empty() {
        return plain(StatusCode::BAD_REQUEST, "");
    }
    if let Some((archive, inner)) = split_zip_path(&path) {
        let staging = match Staging::create() {
            Ok(staging) => staging,
            Err(_) => {
                return plain(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to stage the archive",
                );
            }
        };
        let mut items = Vec::new();
        if collect_archive_entries(&archive, &inner, &staging, &mut items).is_err() {
            return plain(StatusCode::BAD_REQUEST, "Failed to read the archive");
        }
        let folder = if inner.is_empty() {
            archive
                .file_name()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_default()
        } else {
            inner.rsplit('/').next().unwrap_or_default().to_owned()
        };
        let name = if json_name.is_empty() {
            format!("{folder}.zip")
        } else {
            json_name
        };
        return stream_archive(state, items, &name, Some(staging), permit).await;
    }
    let root = PathBuf::from(&path);
    if !root.is_dir() {
        return plain(StatusCode::NOT_FOUND, "");
    }
    let mut items = Vec::new();
    if walk_directory(&root, "", &mut items).is_err() {
        return plain(StatusCode::BAD_REQUEST, "Failed to read the directory");
    }
    let folder = root
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_default();
    let name = if json_name.is_empty() {
        format!("{folder}.zip")
    } else {
        json_name
    };
    stream_archive(state, items, &name, None, permit).await
}

async fn zip_files(
    state: &ServerState,
    params: &Params,
    permit: tokio::sync::OwnedSemaphorePermit,
) -> Response {
    if !desktop_access_allowed(state) {
        return plain(StatusCode::FORBIDDEN, "");
    }
    let Some(plaintext) = decrypt_url_id(state, &params.id) else {
        return plain(
            StatusCode::BAD_REQUEST,
            "File is expired or does not exist.",
        );
    };
    let request: ZipFilesRequest = match serde_json::from_str(&plaintext) {
        Ok(request) => request,
        Err(error) => return plain(StatusCode::BAD_REQUEST, error.to_string()),
    };
    if request.r#type.is_empty() {
        return plain(StatusCode::BAD_REQUEST, "");
    }
    // The item list is a platform search (MediaStore / packages / a temp
    // selection); the host resolves it, Rust owns the archive.
    let facts = match state
        .host
        .call(
            "zipItemsFacts",
            serde_json::json!({
                "type": request.r#type,
                "query": request.query,
                "id": request.id,
            }),
        )
        .await
    {
        Ok(facts) => facts,
        Err(error) => return plain(StatusCode::BAD_REQUEST, error),
    };
    if facts.as_bool() == Some(false) {
        return plain(StatusCode::NOT_FOUND, "");
    }
    let items: Vec<(PathBuf, String)> = facts["items"]
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(|value| {
                    let path = value.get("path")?.as_str()?.to_owned();
                    let name = value
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned();
                    Some((PathBuf::from(path), name))
                })
                .filter(|(path, _)| path.is_file())
                .collect()
        })
        .unwrap_or_default();
    if items.is_empty() {
        return plain(StatusCode::NOT_FOUND, "");
    }
    let name = if request.name.is_empty() {
        "download.zip".to_owned()
    } else {
        request.name
    };
    stream_archive(state, items, &name, None, permit).await
}

pub(crate) async fn dir(
    State(state): State<ServerState>,
    Query(params): Query<Params>,
) -> Response {
    let Some(permit) = state.archives.acquire() else {
        return plain(StatusCode::TOO_MANY_REQUESTS, "");
    };
    zip_dir(&state, &params, permit).await
}

pub(crate) async fn files(
    State(state): State<ServerState>,
    Query(params): Query<Params>,
) -> Response {
    let Some(permit) = state.archives.acquire() else {
        return plain(StatusCode::TOO_MANY_REQUESTS, "");
    };
    zip_files(&state, &params, permit).await
}

#[cfg(test)]
#[path = "../../../tests/unit/http/routes/zip.rs"]
mod tests;
